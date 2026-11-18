// The money an invoice payment is about reaching the cash book.
//
// A payment against an invoice lowers what is owed and writes nothing anywhere else. This is the step that closes that gap,
// and what is tested is that it asks rather than assumes: which account the money landed in comes from the Book's own cash
// and bank accounts and is never guessed, what it clears is the receivable account from the same chart, and the receipt is
// written posted so an account's statement and its balance cannot disagree about the same money.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/take_payment_in_sheet.dart';

Account _account(String number, String name, String type) => Account(
  accountNumber: number,
  accountName: name,
  accountType: type,
  normalBalance: type == 'asset' ? 'debit' : 'credit',
);

final _chart = [
  _account('1000', 'Cash', 'asset'),
  _account('1010', 'Bank', 'asset'),
  _account('1100', 'Accounts Receivable', 'asset'),
  _account('4000', 'Sales', 'revenue'),
];

class _FakeLedger extends AccountingApiService {
  _FakeLedger(this.chart);

  final List<Account> chart;

  @override
  Future<List<Account>> getAllAccounts() async => chart;
}

final _places = [
  {
    'account_code': 'CB-001',
    'account_name': 'Main bank',
    'account_type': 'bank',
    'current_balance': '1150.000000',
    'opening_balance': '1200.000000',
  },
  {
    'account_code': 'CB-002',
    'account_name': 'Petty cash',
    'account_type': 'cash',
    'current_balance': '40.000000',
    'opening_balance': '40.000000',
  },
  {
    'account_code': 'CB-003',
    'account_name': 'Savings pot',
    'account_type': 'savings',
    'current_balance': '500.000000',
    'opening_balance': '500.000000',
  },
];

CashBookService _cash(MockClient client) =>
    CashBookService(base: 'http://engine.test/cashbook', client: client);

MockClient _serving({void Function(Map<String, dynamic>)? onEntry}) =>
    MockClient((request) async {
      if (request.url.path.endsWith('/accounts')) {
        return http.Response(jsonEncode(_places), 200);
      }
      final sent = jsonDecode(request.body) as Map<String, dynamic>;
      onEntry?.call(sent);
      return http.Response(
        jsonEncode({
          'id': 'e7',
          'entry_date': sent['entry_date'],
          'voucher_number': '',
          'description': sent['description'],
          'account_code': sent['account_code'],
          'amount': sent['amount'],
          'is_debit': sent['is_debit'],
          'status': sent['status'],
        }),
        200,
      );
    });

Future<void> _open(WidgetTester tester, CashBookService cash) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: TakePaymentInSheet(
          amount: ExactAmount.parse('455.000000'),
          invoiceNumber: 'INV-0007',
          customer: 'CUST-3',
          cash: cash,
          ledger: _FakeLedger(_chart),
          onDate: '2026-11-18',
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'the payment is taken into an account the person picks, never a guess',
    (tester) async {
      late Map<String, dynamic> sent;
      await _open(tester, _cash(_serving(onEntry: (body) => sent = body)));

      await tester.tap(find.byType(DropdownButtonFormField<String>).first);
      await tester.pumpAndSettle();
      // a savings pot is not somewhere money was paid into, so it is deliberately not offered
      expect(find.textContaining('Savings pot'), findsNothing);
      await tester.tap(find.textContaining('Petty cash').last);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Take it in'));
      await tester.pumpAndSettle();

      expect(sent['bank_account'], 'CB-002');
    },
  );

  testWidgets(
    'the receipt is posted, clears the receivable, and says what it was for',
    (tester) async {
      late Map<String, dynamic> sent;
      await _open(tester, _cash(_serving(onEntry: (body) => sent = body)));

      expect(find.textContaining('Take in 455.00'), findsOneWidget);
      await tester.tap(find.text('Take it in'));
      await tester.pumpAndSettle();

      expect(sent['is_debit'], isTrue);
      expect(sent['status'], 'posted');
      expect(sent['account_code'], '1100');
      expect(sent['amount'], '455.000000');
      expect(sent['description'], 'INV-0007 paid by CUST-3');
      expect(sent['entry_date'], '2026-11-18');
    },
  );

  testWidgets(
    'a Book with no cash or bank account is told, and nothing is sent',
    (tester) async {
      var sent = false;
      final cash = _cash(
        MockClient((request) async {
          if (request.url.path.endsWith('/accounts')) {
            return http.Response(jsonEncode([_places.last]), 200);
          }
          sent = true;
          return http.Response('{}', 200);
        }),
      );
      await _open(tester, cash);

      await tester.tap(find.text('Take it in'));
      await tester.pumpAndSettle();

      expect(find.textContaining('no cash or bank account'), findsOneWidget);
      expect(sent, isFalse);
    },
  );

  testWidgets(
    'a chart with no receivable account is refused rather than posted to a guess',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: TakePaymentInSheet(
              amount: ExactAmount.parse('455.000000'),
              invoiceNumber: 'INV-0007',
              customer: 'CUST-3',
              cash: _cash(_serving()),
              ledger: _FakeLedger([_account('1000', 'Cash', 'asset')]),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Take it in'));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('no accounts-receivable account'),
        findsOneWidget,
      );
    },
  );
}
