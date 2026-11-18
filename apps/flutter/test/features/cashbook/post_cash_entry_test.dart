// One receipt or payment reaching the ledger.
//
// The cash book and the ledger are two records of the same money, and nothing keeps them in step by itself: the cash book
// moves a balance, the ledger holds the double entry. What is tested here is that the phone writes the entry the money
// actually is - the right way round for money in and money out - out of the accounts the Book already has, refuses when a
// side cannot be named, and carries the cash entry's own id as the reference so the same money cannot be posted twice.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/post_cash_entry_sheet.dart';

Account _account(String number, String name, String type) => Account(
  accountNumber: number,
  accountName: name,
  accountType: type,
  normalBalance: type == 'asset' ? 'debit' : 'credit',
);

final _chart = [
  _account('1000', 'Cash', 'asset'),
  _account('1100', 'Accounts Receivable', 'asset'),
  _account('4000', 'Sales', 'revenue'),
  _account('6100', 'Rent expense', 'expense'),
];

class _FakeLedger extends AccountingApiService {
  _FakeLedger(this.chart);

  final List<Account> chart;
  final List<JournalEntry> written = [];
  String? refusal;

  @override
  Future<List<Account>> getAllAccounts() async => chart;

  @override
  Future<JournalEntry> createJournalEntry(JournalEntry entry) async {
    final no = refusal;
    if (no != null) throw Exception(no);
    written.add(entry);
    return entry;
  }
}

final _bank = CashAccount.fromJson({
  'id': 'a1',
  'account_code': 'CB-001',
  'account_name': 'Main bank',
  'account_type': 'bank',
  'currency': 'USD',
  'opening_balance': '1200.000000',
  'current_balance': '1150.000000',
});

CashEntry _entry({
  required bool in_,
  String against = '1100',
  String description = 'Paid by Acme',
  String id = 'e9f1',
}) => CashEntry(
  id: id,
  date: '2026-11-18',
  voucher: 'RV-0004',
  description: description,
  moneyIn: in_,
  amount: ExactAmount.parse('250.000000'),
  status: 'pending',
  againstAccount: against,
);

Future<_FakeLedger> _open(
  WidgetTester tester,
  CashEntry entry, {
  List<Account>? chart,
}) async {
  final ledger = _FakeLedger(chart ?? _chart);
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: PostCashEntrySheet(entry: entry, account: _bank, ledger: ledger),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return ledger;
}

void main() {
  testWidgets('money in is Dr what it landed in, Cr what it was for', (
    tester,
  ) async {
    final ledger = await _open(tester, _entry(in_: true));

    expect(find.textContaining('Dr 1000 Cash'), findsOneWidget);
    expect(find.textContaining('Cr 1100 Accounts Receivable'), findsOneWidget);

    await tester.tap(find.text('Post it'));
    await tester.pumpAndSettle();

    final entry = ledger.written.single;
    expect(entry.referenceNumber, 'CB-e9f1');
    expect(entry.status, 'posted');
    expect(entry.lines[0].accountNumber, '1000');
    expect(entry.lines[0].debit, ExactAmount.parse('250.000000'));
    expect(entry.lines[1].accountNumber, '1100');
    expect(entry.lines[1].credit, ExactAmount.parse('250.000000'));
  });

  testWidgets('money out is Dr what it was spent on, Cr what it left', (
    tester,
  ) async {
    final ledger = await _open(
      tester,
      _entry(in_: false, against: '6100', description: 'Rent for November'),
    );

    expect(find.textContaining('Dr 6100 Rent expense'), findsOneWidget);
    expect(find.textContaining('Cr 1000 Cash'), findsOneWidget);

    await tester.tap(find.text('Post it'));
    await tester.pumpAndSettle();

    final entry = ledger.written.single;
    expect(entry.lines[0].accountNumber, '6100');
    expect(entry.lines[0].debit, ExactAmount.parse('250.000000'));
    expect(entry.lines[1].accountNumber, '1000');
    expect(entry.lines[1].credit, ExactAmount.parse('250.000000'));
  });

  testWidgets(
    'an entry whose other side is not in the chart is refused, not guessed',
    (tester) async {
      final ledger = await _open(tester, _entry(in_: true, against: '9999'));

      await tester.tap(find.text('Post it'));
      await tester.pumpAndSettle();

      expect(find.textContaining('Choose both accounts'), findsOneWidget);
      expect(ledger.written, isEmpty);
    },
  );

  testWidgets(
    'a Book without a cash or bank account in its chart is not posted anywhere',
    (tester) async {
      final ledger = await _open(
        tester,
        _entry(in_: true),
        chart: [
          _account('1100', 'Accounts Receivable', 'asset'),
          _account('4000', 'Sales', 'revenue'),
        ],
      );

      expect(find.textContaining('Not suggested by the chart'), findsWidgets);
      await tester.tap(find.text('Post it'));
      await tester.pumpAndSettle();
      expect(ledger.written, isEmpty);
    },
  );

  testWidgets('a refusal from the engine is shown as it was said', (
    tester,
  ) async {
    final ledger = await _open(tester, _entry(in_: true));
    ledger.refusal =
        'Journal Entry with reference CB-e9f1 already exists for source MobileApp.';

    await tester.tap(find.text('Post it'));
    await tester.pumpAndSettle();

    expect(
      find.textContaining('already exists for source MobileApp'),
      findsOneWidget,
    );
  });
}
