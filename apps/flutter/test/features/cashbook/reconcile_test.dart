// Checking the cash book against the bank's own statement.
//
// This is the only place the books meet somebody outside them, so the tests care about the two things that make a check
// worth having: the gap is said in the direction it actually runs (bank ahead, or books ahead), and a check is kept as it
// stood on the day rather than quietly passing because the figures were retyped until they matched.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/cash_account_page.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/reconcile_sheet.dart';

const _accounts = [
  {
    'id': 'a1',
    'account_code': 'CB-001',
    'account_name': 'Main bank',
    'account_type': 'bank',
    'currency': 'USD',
    'opening_balance': '1200.000000',
    'current_balance': '1150.000000',
  },
];

final _bank = CashAccount.fromJson(Map<String, dynamic>.from(_accounts.first));

Map<String, dynamic> _check({
  required String statement,
  String books = '1150.000000',
  String status = 'in_progress',
  String id = 'r1',
}) => {
  'id': id,
  'bank_account': 'CB-001',
  'statement_date': '2026-11-30',
  'statement_balance': statement,
  'book_balance': books,
  'status': status,
};

CashBookService _service(MockClient client) =>
    CashBookService(base: 'http://engine.test/cashbook', client: client);

void main() {
  testWidgets('a statement that matches is said to agree', (tester) async {
    final service = _service(
      MockClient((_) async => http.Response('{"ok":true}', 200)),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ReconcileSheet(account: _bank, service: service),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('books say 1150.00'), findsOneWidget);
    await tester.enterText(
      find.widgetWithText(TextField, 'Statement balance'),
      '1150',
    );
    await tester.pumpAndSettle();
    expect(find.text('They agree. Nothing to chase.'), findsOneWidget);
  });

  testWidgets('a bank ahead of the books is said that way round', (
    tester,
  ) async {
    final service = _service(
      MockClient((_) async => http.Response('{"ok":true}', 200)),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ReconcileSheet(account: _bank, service: service),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.enterText(
      find.widgetWithText(TextField, 'Statement balance'),
      '1300',
    );
    await tester.pumpAndSettle();

    expect(
      find.textContaining('The bank shows 150.00 more than your books'),
      findsOneWidget,
    );
  });

  testWidgets(
    'the check is recorded with the statement balance as given, gap and all',
    (tester) async {
      late Map<String, dynamic> sent;
      final service = _service(
        MockClient((request) async {
          sent = jsonDecode(request.body) as Map<String, dynamic>;
          return http.Response(
            jsonEncode(_check(statement: sent['statement_balance'] as String)),
            200,
          );
        }),
      );
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ReconcileSheet(account: _bank, service: service),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.enterText(
        find.widgetWithText(TextField, 'Statement balance'),
        '1120.50',
      );
      await tester.tap(find.text('Record the check'));
      await tester.pumpAndSettle();

      expect(sent['bank_account'], 'CB-001');
      expect(sent['statement_balance'], '1120.500000');
      expect(sent['statement_date'], isNotEmpty);
    },
  );

  test('a completed check names who checked it', () async {
    late Map<String, dynamic> sent;
    final service = _service(
      MockClient((request) async {
        sent = jsonDecode(request.body) as Map<String, dynamic>;
        return http.Response(
          jsonEncode(_check(statement: '1150.000000', status: 'completed')),
          200,
        );
      }),
    );
    final check = CashReconciliation.fromJson(_check(statement: '1150.000000'));

    final done = await service.markReconciled(
      check,
      reviewedBy: 'Samuel',
      completedAt: '2026-11-18T09:00:00Z',
    );

    expect(sent['reviewed_by'], 'Samuel');
    expect(done.status, 'completed');
  });

  test('a check with nobody named is refused before it is sent', () async {
    final service = _service(
      MockClient((_) async => fail('nothing may be sent')),
    );
    await expectLater(
      service.markReconciled(
        CashReconciliation.fromJson(_check(statement: '1150.000000')),
        reviewedBy: ' ',
        completedAt: '2026-11-18T09:00:00Z',
      ),
      throwsA(
        predicate((failure) => failure.toString().contains('who checked it')),
      ),
    );
  });

  testWidgets('the account page says when a month disagreed', (tester) async {
    final service = _service(
      MockClient((request) async {
        if (request.url.path.endsWith('/accounts')) {
          return http.Response(jsonEncode(_accounts), 200);
        }
        if (request.url.path.endsWith('/reconciliations')) {
          return http.Response(
            jsonEncode([_check(statement: '1120.500000')]),
            200,
          );
        }
        return http.Response('[]', 200);
      }),
    );

    await tester.pumpWidget(
      MaterialApp(
        home: CashAccountPage(account: _bank, service: service),
      ),
    );
    await tester.pumpAndSettle();

    expect(
      find.textContaining('bank 1120.50 · books 1150.00 · books 29.50 ahead'),
      findsOneWidget,
    );
    expect(find.textContaining('still to be checked'), findsOneWidget);
  });

  test('the gap runs the way the difference runs', () {
    final behind = CashReconciliation.fromJson(
      _check(statement: '1300.000000'),
    );
    expect(behind.gap.isNegative, isTrue);
    expect(reconciliationLine(behind), contains('bank 150.00 ahead'));

    final level = CashReconciliation.fromJson(_check(statement: '1150.000000'));
    expect(level.agrees, isTrue);
    expect(reconciliationLine(level), contains('agreed'));
  });
}
