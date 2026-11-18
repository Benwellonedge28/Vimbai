// The cash book: what it reads, what it sends, and the one payment it refuses to send.
//
// The refusal is the important test. The engine moves a balance by whatever it is told, so a payment larger than the money in
// the account would be carried out faithfully and leave the book saying the account owes itself. The phone checks it first.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/cash_account_page.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/cash_book_page.dart';

const _accounts = [
  {
    'id': 'a1',
    'account_code': 'CB-001',
    'account_name': 'Main bank',
    'account_type': 'bank',
    'bank_name': 'CBZ',
    'currency': 'USD',
    'opening_balance': '1200.000000',
    'current_balance': '900.000000',
  },
];

const _position = {
  'total_cash': '0.000000',
  'total_bank': '900.000000',
  'total_savings': '50.000000',
  // the engine answers this as the money in *current* accounts only, which was 0.00 on a live Book whose balance sat in one
  // bank account - so what the page leads with must not be this field
  'total_current': '0.000000',
};

const _entries = [
  {
    'id': 'e1',
    'entry_date': '2026-11-18',
    'voucher_number': 'PV-0001',
    'description': 'Rent',
    'is_debit': false,
    'amount': '300.000000',
    'status': 'pending',
  },
];

void main() {
  testWidgets('the page leads with what is held, and names each account', (
    tester,
  ) async {
    final service = CashBookService(
      base: 'http://engine.test/cashbook',
      client: MockClient((request) async {
        if (request.url.path.endsWith('/cash-position')) {
          return http.Response(jsonEncode(_position), 200);
        }
        return http.Response(jsonEncode(_accounts), 200);
      }),
    );

    await tester.pumpWidget(MaterialApp(home: CashBookPage(service: service)));
    await tester.pumpAndSettle();

    expect(find.text('950.00 held in total'), findsOneWidget);
    expect(find.textContaining('900.00 bank'), findsOneWidget);
    expect(find.textContaining('50.00 savings'), findsOneWidget);
    expect(find.text('CB-001 · Main bank'), findsOneWidget);
    expect(find.text('900.00'), findsOneWidget);
  });

  testWidgets(
    'a read that fails says so, and never reads as an empty cash book',
    (tester) async {
      final service = CashBookService(
        base: 'http://engine.test/cashbook',
        client: MockClient(
          (_) async => http.Response('{"error":"No active membership"}', 403),
        ),
      );

      await tester.pumpWidget(
        MaterialApp(home: CashBookPage(service: service)),
      );
      await tester.pumpAndSettle();

      expect(find.byType(CouldNotRead), findsOneWidget);
      expect(find.textContaining('No cash or bank account yet'), findsNothing);
    },
  );

  testWidgets('money in is sent as a receipt against the account', (
    tester,
  ) async {
    late Map<String, dynamic> sent;
    final service = CashBookService(
      base: 'http://engine.test/cashbook',
      client: MockClient((request) async {
        if (request.method == 'POST') {
          sent = jsonDecode(request.body) as Map<String, dynamic>;
          return http.Response(
            jsonEncode({
              'id': 'e2',
              'entry_date': '2026-11-18',
              'description': 'Paid by Acme',
              'is_debit': true,
              'amount': '250.000000',
              'status': 'pending',
            }),
            200,
          );
        }
        if (request.url.path.endsWith('/accounts')) {
          return http.Response(jsonEncode(_accounts), 200);
        }
        return http.Response(jsonEncode(_entries), 200);
      }),
    );
    final account = CashAccount.fromJson(
      Map<String, dynamic>.from(_accounts.first),
    );

    await tester.pumpWidget(
      MaterialApp(
        home: CashAccountPage(account: account, service: service),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('900.00 in this account'), findsOneWidget);
    expect(find.textContaining('300.00 out'), findsOneWidget);

    await tester.tap(find.text('Money in'));
    await tester.pumpAndSettle();
    await tester.enterText(find.widgetWithText(TextField, 'Amount'), '250');
    await tester.enterText(
      find.widgetWithText(TextField, 'What for'),
      'Paid by Acme',
    );
    await tester.tap(find.text('Record it'));
    await tester.pumpAndSettle();

    expect(sent['is_debit'], isTrue);
    expect(sent['book_type'], 'receipts');
    expect(sent['bank_account'], 'CB-001');
    expect(sent['amount'], '250.000000');
    expect(sent['description'], 'Paid by Acme');
    // the engine moves the balance on creation whatever the status, but counts only posted entries in the account's own
    // statement and in a reconciliation - an entry left pending made those two halves disagree
    expect(sent['status'], 'posted');
  });

  testWidgets(
    'a payment bigger than the money in the account is refused, not sent',
    (tester) async {
      var wrote = 0;
      final service = CashBookService(
        base: 'http://engine.test/cashbook',
        client: MockClient((request) async {
          if (request.method == 'POST') wrote++;
          if (request.url.path.endsWith('/accounts')) {
            return http.Response(jsonEncode(_accounts), 200);
          }
          return http.Response(jsonEncode(_entries), 200);
        }),
      );
      final account = CashAccount.fromJson(
        Map<String, dynamic>.from(_accounts.first),
      );

      await tester.pumpWidget(
        MaterialApp(
          home: CashAccountPage(account: account, service: service),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Money out'));
      await tester.pumpAndSettle();
      await tester.enterText(find.widgetWithText(TextField, 'Amount'), '1000');
      await tester.enterText(
        find.widgetWithText(TextField, 'What for'),
        'Rent',
      );
      await tester.tap(find.text('Record it'));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('more than the 900.00 in this account'),
        findsOneWidget,
      );
      expect(
        wrote,
        0,
        reason: 'the engine would have carried this out faithfully',
      );
    },
  );

  test('an entry of nothing is refused before it leaves the phone', () async {
    final service = CashBookService(
      base: 'http://engine.test/cashbook',
      client: MockClient((_) async {
        fail('nothing may be sent');
      }),
    );
    await expectLater(
      service.record(
        account: CashAccount.fromJson(
          Map<String, dynamic>.from(_accounts.first),
        ),
        moneyIn: true,
        amount: ExactAmount.zero,
        description: 'Nothing',
        date: '2026-11-18',
      ),
      throwsA(
        predicate(
          (failure) => failure.toString().contains('more than nothing'),
        ),
      ),
    );
  });
}
