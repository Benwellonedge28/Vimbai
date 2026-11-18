// One supplier's account, held to the ledger it came from.
//
// The point of the page is that it never disagrees with the balance on "What you owe": the closing figure is the engine's
// own, and the page prints it rather than adding the lines up itself. These tests pin that, and pin the two ways a page
// like this lies to somebody: showing a read that failed as an empty account, and dressing a movement the ledger does not
// count (a discount) up as if it had moved money.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/payables/data/supplier_account.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/supplier_account_page.dart';

const String _account =
    '{"creditor_id":"paperco","creditor_name":"Paper Co",'
    '"lines":['
    '{"date":"2026-10-01","transaction_type":"purchase_invoice","moves":true,"reference":"INV-77","amount":"500.000000","running":"500.000000"},'
    '{"date":"2026-10-20","transaction_type":"payment","moves":true,"reference":"EFT 12","amount":"200.000000","running":"300.000000"},'
    '{"date":"2026-11-05","transaction_type":"discount","moves":false,"reference":"","amount":"50.000000","running":"300.000000"}'
    '],"total_invoices":"500.000000","total_debit_notes":"0.000000","total_payments":"200.000000",'
    '"total_refunds":"0.000000","balance":"300.000000","transaction_count":3,"last_activity":"2026-11-05"}';

const String _settled =
    '{"creditor_id":"inkco","creditor_name":"Ink Supplies",'
    '"lines":[{"date":"2026-11-01","transaction_type":"purchase_invoice","moves":true,"reference":"","amount":"95.500000","running":"95.500000"},'
    '{"date":"2026-11-12","transaction_type":"payment","moves":true,"reference":"","amount":"95.500000","running":"0.000000"}],'
    '"total_invoices":"95.500000","total_debit_notes":"0.000000","total_payments":"95.500000",'
    '"total_refunds":"0.000000","balance":"0.000000","transaction_count":2,"last_activity":"2026-11-12"}';

const String _empty =
    '{"creditor_id":"nobody","creditor_name":"nobody","lines":[],"total_invoices":"0.000000",'
    '"total_debit_notes":"0.000000","total_payments":"0.000000","total_refunds":"0.000000",'
    '"balance":"0.000000","transaction_count":0,"last_activity":""}';

/// A service answering from a canned engine answer, and everything it was asked.
class _Fake {
  _Fake({String? answer, int status = 200})
    : _answer = answer,
      _status = status;

  final String? _answer;
  final int _status;

  final List<http.Request> sent = [];

  late final SupplierAccountService service = SupplierAccountService.fresh()
    ..useClient(
      MockClient((request) async {
        sent.add(request);
        if (_status != 200) {
          return http.Response('{"detail":"no such supplier"}', _status);
        }
        return http.Response(_answer ?? _account, 200);
      }),
    );
}

/// A ledger that answers a chart and records what was posted, without a server.
class _FakeLedger extends AccountingApiService {
  _FakeLedger({List<Account>? chart}) : _chart = chart ?? _usualChart();

  static List<Account> _usualChart() => [
    Account(
      id: 'a1',
      accountNumber: '1000',
      accountName: 'Cash',
      accountType: 'asset',
      normalBalance: 'debit',
    ),
    Account(
      id: 'a2',
      accountNumber: '2000',
      accountName: 'Accounts Payable',
      accountType: 'liability',
      normalBalance: 'credit',
    ),
    Account(
      id: 'a3',
      accountNumber: '5000',
      accountName: 'Purchases',
      accountType: 'expense',
      normalBalance: 'debit',
    ),
  ];

  final List<Account> _chart;
  final List<JournalEntry> posted = [];

  @override
  Future<List<Account>> getAllAccounts() async => _chart;

  @override
  Future<JournalEntry> createJournalEntry(JournalEntry entry) async {
    posted.add(entry);
    return entry;
  }
}

void main() {
  test(
    'the account reads its closing figure from the engine, not from its lines',
    () async {
      // the engine says 300.00 while the lines would also sum to 300.00; the point is that the figure printed is the
      // engine's, so a disagreement is impossible
      final account = SupplierAccount.fromJson(
        jsonDecode(_account) as Map<String, dynamic>,
      );
      expect(account.balance.toDecimalString(), '300.00');
      expect(account.billed.toDecimalString(), '500.00');
      expect(account.paid.toDecimalString(), '200.00');
      expect(account.lines.length, 3);
      expect(account.headline, 'You owe Paper Co 300.00');
    },
  );

  test(
    'a settled supplier is called settled rather than shown owing nothing',
    () async {
      final account = SupplierAccount.fromJson(
        jsonDecode(_settled) as Map<String, dynamic>,
      );
      expect(account.headline, 'Settled: nothing owing');
    },
  );

  test(
    'an account with no movements says so instead of reading as zero owed',
    () async {
      final account = SupplierAccount.fromJson(
        jsonDecode(_empty) as Map<String, dynamic>,
      );
      expect(account.isEmpty, isTrue);
      expect(account.headline, 'Nothing on this account yet');
    },
  );

  test('a discount is worded as moving nothing, never as a payment', () async {
    final account = SupplierAccount.fromJson(
      jsonDecode(_account) as Map<String, dynamic>,
    );
    final discount = account.lines.last;
    expect(discount.kind, 'discount');
    expect(discount.moves, isFalse);
    expect(discount.wording, contains('moves nothing'));
  });

  test(
    'the account is asked for by supplier, on the purchases ledger',
    () async {
      final fake = _Fake();
      await fake.service.account('paperco');
      expect(
        fake.sent.single.url.path,
        endsWith('/purchases-ledger-control/creditors/paperco'),
      );
    },
  );

  test(
    'an account with no supplier named is refused before anything is sent',
    () async {
      final fake = _Fake();
      await expectLater(
        fake.service.account('  '),
        throwsA(predicate((failure) => '$failure'.contains('which supplier'))),
      );
      expect(fake.sent, isEmpty);
    },
  );

  testWidgets('the page leads with what is owed and lists every movement', (
    tester,
  ) async {
    final fake = _Fake();
    await tester.pumpWidget(
      MaterialApp(
        home: SupplierAccountPage(
          supplierId: 'paperco',
          supplierName: 'Paper Co',
          service: fake.service,
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('You owe Paper Co 300.00'), findsOneWidget);
    expect(find.textContaining('500.00 billed'), findsOneWidget);
    expect(find.text('Bill received'), findsOneWidget);
    expect(find.text('Paid them'), findsOneWidget);
    expect(find.text('Discount agreed, moves nothing'), findsOneWidget);
    // the reference travels with the line, so a person can match it to the paperwork
    expect(find.text('INV-77'), findsOneWidget);
  });

  testWidgets(
    'a supplier with nothing on the books says so rather than failing',
    (tester) async {
      final fake = _Fake(answer: _empty);
      await tester.pumpWidget(
        MaterialApp(
          home: SupplierAccountPage(
            supplierId: 'nobody',
            service: fake.service,
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(
        find.text('Nothing has passed between you and this supplier yet.'),
        findsOneWidget,
      );
    },
  );

  testWidgets('a bill can be put into the books from the account', (
    tester,
  ) async {
    final fake = _Fake();
    final ledger = _FakeLedger();
    await tester.pumpWidget(
      MaterialApp(
        home: SupplierAccountPage(
          supplierId: 'paperco',
          supplierName: 'Paper Co',
          service: fake.service,
          ledger: ledger,
        ),
      ),
    );
    await tester.pumpAndSettle();

    // the bill line offers it; the discount line does not, because it moves nothing
    expect(find.text('Post to books'), findsNWidgets(2));
    await tester.tap(find.text('Bill received'));
    await tester.pumpAndSettle();

    expect(find.text('Post this bill'), findsOneWidget);
    expect(find.textContaining('Dr 5000 Purchases'), findsOneWidget);
    expect(find.textContaining('Cr 2000 Accounts Payable'), findsOneWidget);
    expect(ledger.posted, isEmpty);

    await tester.tap(find.text('Post it'));
    await tester.pumpAndSettle();

    final entry = ledger.posted.single;
    expect(entry.referenceNumber, 'INV-77');
    expect(entry.status, 'posted');
    expect(entry.lines.first.accountNumber, '5000');
    expect(entry.lines.first.debit.toDecimalString(), '500.00');
    expect(entry.lines.last.accountNumber, '2000');
    expect(entry.lines.last.credit.toDecimalString(), '500.00');
  });

  testWidgets(
    'a payment posted from the account clears the payable and leaves cash',
    (tester) async {
      final fake = _Fake();
      final ledger = _FakeLedger();
      await tester.pumpWidget(
        MaterialApp(
          home: SupplierAccountPage(
            supplierId: 'paperco',
            supplierName: 'Paper Co',
            service: fake.service,
            ledger: ledger,
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Paid them'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Post it'));
      await tester.pumpAndSettle();

      final entry = ledger.posted.single;
      expect(entry.description, 'Paid Paper Co');
      expect(entry.lines.first.accountNumber, '2000');
      expect(entry.lines.first.debit.toDecimalString(), '200.00');
      expect(entry.lines.last.accountNumber, '1000');
      expect(entry.lines.last.credit.toDecimalString(), '200.00');
    },
  );

  testWidgets('a chart with no payable account refuses and posts nothing', (
    tester,
  ) async {
    final fake = _Fake();
    final ledger = _FakeLedger(
      chart: [
        Account(
          id: 'a1',
          accountNumber: '1000',
          accountName: 'Cash',
          accountType: 'asset',
          normalBalance: 'debit',
        ),
      ],
    );
    await tester.pumpWidget(
      MaterialApp(
        home: SupplierAccountPage(
          supplierId: 'paperco',
          supplierName: 'Paper Co',
          service: fake.service,
          ledger: ledger,
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Bill received'));
    await tester.pumpAndSettle();

    expect(find.textContaining('no Accounts Payable account'), findsOneWidget);
    expect(find.text('Post it'), findsNothing);
    expect(ledger.posted, isEmpty);
  });

  testWidgets('a failed read is never shown as an empty account', (
    tester,
  ) async {
    final fake = _Fake(status: 500);
    await tester.pumpWidget(
      MaterialApp(
        home: SupplierAccountPage(
          supplierId: 'paperco',
          supplierName: 'Paper Co',
          service: fake.service,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      find.text('Nothing has passed between you and this supplier yet.'),
      findsNothing,
    );
    expect(find.textContaining('could not'), findsWidgets);
  });
}
