// A cost used but not billed, and money paid for something not yet used.
//
// What these pin: that the month and the cost are asked for plainly and the estimate carries no tax, that taking an
// estimate back is said to be a reversal before it happens, that a prepayment states what is still to be used and offers the
// earliest month it has not used, and that the engine's own refusals reach the person in words they can act on.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accruals/data/accrual_posting.dart';
import 'package:vimbai_mobile_client/features/accruals/presentation/a_cost_not_yet_billed_page.dart';
import 'package:vimbai_mobile_client/features/accruals/presentation/paid_up_front_page.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';

/// The Book's chart: where costs land, and the account money sits in.
class _FakeLedger extends AccountingApiService {
  @override
  Future<List<Account>> getAllAccounts() async => [
    Account(
      accountNumber: '1010',
      accountName: 'Bank',
      accountType: 'asset',
      normalBalance: 'debit',
    ),
    Account(
      accountNumber: '1300',
      accountName: 'Prepaid Expenses',
      accountType: 'asset',
      normalBalance: 'debit',
    ),
    Account(
      accountNumber: '6100',
      accountName: 'Electricity',
      accountType: 'expense',
      normalBalance: 'debit',
    ),
    Account(
      accountNumber: '6300',
      accountName: 'Insurance',
      accountType: 'expense',
      normalBalance: 'debit',
    ),
  ];
}

/// The money the business holds.
class _FakeCashBook extends CashBookService {
  @override
  Future<List<CashAccount>> accounts() async => [
    CashAccount(
      id: 'cash-1',
      code: 'BANK',
      name: 'Bank',
      type: 'bank',
      bankName: 'Bank',
      currency: 'USD',
      openingBalance: ExactAmount.zero,
      balance: ExactAmount.zero,
    ),
  ];
}

/// Stands in for the engine's period-end journals: it remembers what it was asked to do.
class _FakeAccruals extends AccrualPostingService {
  _FakeAccruals({
    this.standing = const [],
    this.paid = const [],
    this.refusal = '',
  }) : super.fresh();

  List<Accrual> standing;
  List<Prepayment> paid;

  /// When set, the next write is refused in these words, as the engine would.
  String refusal;

  final List<Map<String, Object?>> recorded = [];
  final List<String> takenBack = [];
  final List<String> usedUp = [];

  @override
  Future<List<Accrual>> accruals({String period = ''}) async => standing;

  @override
  Future<Accrual> recordAccrual({
    required String period,
    required String expenseAccount,
    required ExactAmount amount,
    String description = '',
    String notes = '',
  }) async {
    if (refusal.isNotEmpty) throw Exception(refusal);
    recorded.add({
      'period': period,
      'account': expenseAccount,
      'amount': amount.toDecimalString(),
      'what': description,
    });
    final made = Accrual(
      id: 'accrual-1',
      period: period,
      expenseAccount: expenseAccount,
      accruedAccount: '2200',
      amount: amount.toDecimalString(),
      description: description,
      journalEntryId: 'entry-1',
      reversalEntryId: '',
      reversedOn: '',
    );
    standing = [...standing, made];
    return made;
  }

  @override
  Future<Accrual> reverseAccrual(String accrualId) async {
    if (refusal.isNotEmpty) throw Exception(refusal);
    takenBack.add(accrualId);
    final reversed = [
      for (final one in standing)
        if (one.id == accrualId)
          Accrual(
            id: one.id,
            period: one.period,
            expenseAccount: one.expenseAccount,
            accruedAccount: one.accruedAccount,
            amount: one.amount,
            description: one.description,
            journalEntryId: one.journalEntryId,
            reversalEntryId: 'entry-2',
            reversedOn: '2026-12-01',
          )
        else
          one,
    ];
    standing = reversed;
    return reversed.firstWhere((one) => one.id == accrualId);
  }

  @override
  Future<List<Prepayment>> prepayments() async => paid;

  @override
  Future<Prepayment> recordPrepayment({
    required String reference,
    required ExactAmount total,
    required int months,
    required String coversFrom,
    required String paidFrom,
    required String moneyAccount,
    required String expenseAccount,
    String description = '',
    String notes = '',
  }) async {
    if (refusal.isNotEmpty) throw Exception(refusal);
    recorded.add({
      'reference': reference,
      'total': total.toDecimalString(),
      'months': months,
      'covers_from': coversFrom,
      'paid_from': paidFrom,
      'money_account': moneyAccount,
      'account': expenseAccount,
    });
    return _prepayment(
      id: 'prepayment-1',
      reference: reference,
      total: total.toDecimalString(),
      months: months,
      released: const [],
      stillToUse: total.toDecimalString(),
    );
  }

  @override
  Future<Prepayment> releaseMonth({
    required String prepaymentId,
    required String period,
  }) async {
    if (refusal.isNotEmpty) throw Exception(refusal);
    usedUp.add(period);
    return _prepayment(
      id: prepaymentId,
      reference: 'INS-1',
      total: '1200.00',
      months: 12,
      released: [period],
      stillToUse: '1100.00',
    );
  }
}

Prepayment _prepayment({
  required String id,
  required String reference,
  required String total,
  required int months,
  required List<String> released,
  required String stillToUse,
}) => Prepayment(
  id: id,
  reference: reference,
  description: 'Insurance',
  total: total,
  months: months,
  coversFrom: '2026-01',
  prepaidAccount: '1300',
  expenseAccount: '6300',
  stillToUse: stillToUse,
  schedule: [
    const MonthShare(period: '2026-01', amount: '100.00'),
    const MonthShare(period: '2026-02', amount: '100.00'),
  ],
  released: released,
);

Accrual _accrual({
  required String id,
  required String period,
  required String amount,
  String description = 'Electricity',
  String reversalEntryId = '',
  String reversedOn = '',
}) => Accrual(
  id: id,
  period: period,
  expenseAccount: '6100',
  accruedAccount: '2200',
  amount: amount,
  description: description,
  journalEntryId: 'entry-$id',
  reversalEntryId: reversalEntryId,
  reversedOn: reversedOn,
);

Future<void> _openAccruals(
  WidgetTester tester, {
  required _FakeAccruals accruals,
}) async {
  tester.view.physicalSize = const Size(1000, 2600);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      home: ACostNotYetBilledPage(posting: accruals, ledger: _FakeLedger()),
    ),
  );
  await tester.pumpAndSettle();
}

Future<void> _openPrepayments(
  WidgetTester tester, {
  required _FakeAccruals accruals,
}) async {
  tester.view.physicalSize = const Size(1000, 2600);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      home: PaidUpFrontPage(
        posting: accruals,
        ledger: _FakeLedger(),
        cashbook: _FakeCashBook(),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

Future<void> _fill(WidgetTester tester, String label, String text) async {
  await tester.enterText(find.widgetWithText(TextField, label), text);
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('an accrual says what it will do before it is recorded', (
    tester,
  ) async {
    final accruals = _FakeAccruals();
    await _openAccruals(tester, accruals: accruals);

    expect(
      find.textContaining('takes the cost now and owes it'),
      findsOneWidget,
    );
    expect(find.textContaining('this estimate is taken back'), findsOneWidget);
    // nothing is posted by opening the screen
    expect(accruals.recorded, isEmpty);
  });

  testWidgets(
    'the month and the cost are asked for plainly and the estimate carries no tax',
    (tester) async {
      final accruals = _FakeAccruals();
      await _openAccruals(tester, accruals: accruals);

      // the month proposed is the one just gone, because a cost is accrued once its month is over
      final today = DateTime.now();
      final last = DateTime(today.year, today.month - 1, 1);
      final month = '${last.year}-${last.month.toString().padLeft(2, '0')}';
      expect(find.text(month), findsOneWidget);

      await tester.tap(find.text('Choose the cost'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('6100 - Electricity').last);
      await tester.pumpAndSettle();
      await _fill(tester, 'What the month used', '480.00');
      await _fill(tester, 'What it was for', 'Electricity');
      await tester.tap(find.text('Charge the month and owe it'));
      await tester.pumpAndSettle();

      expect(accruals.recorded.length, 1);
      expect(accruals.recorded.first['period'], month);
      expect(accruals.recorded.first['account'], '6100');
      expect(accruals.recorded.first['amount'], '480.00');
      // no tax travels with an estimate: the bill brings its own
      expect(accruals.recorded.first.containsKey('tax'), isFalse);
    },
  );

  testWidgets('taking an estimate back says it is a reversal, not an edit', (
    tester,
  ) async {
    final accruals = _FakeAccruals(
      standing: [_accrual(id: 'a1', period: '2026-11', amount: '480.00')],
    );
    await _openAccruals(tester, accruals: accruals);

    expect(find.textContaining('480.00 owed'), findsOneWidget);
    await tester.tap(find.text('Take it back'));
    await tester.pumpAndSettle();

    expect(find.text('Take the estimate back?'), findsOneWidget);
    expect(
      find.textContaining('stays exactly as it was recorded'),
      findsOneWidget,
    );
    await tester.tap(find.widgetWithText(FilledButton, 'Take it back'));
    await tester.pumpAndSettle();

    expect(accruals.takenBack, ['a1']);
    // once taken back it is listed apart, with the day the reversal was dated
    expect(find.textContaining('Reversed on 2026-12-01'), findsOneWidget);
  });

  testWidgets('an estimate already taken back offers nothing to take back', (
    tester,
  ) async {
    final accruals = _FakeAccruals(
      standing: [
        _accrual(
          id: 'a1',
          period: '2026-11',
          amount: '480.00',
          reversalEntryId: 'entry-2',
          reversedOn: '2026-12-01',
        ),
      ],
    );
    await _openAccruals(tester, accruals: accruals);

    expect(find.text('Take it back'), findsNothing);
    expect(
      find.textContaining('No month is carrying an estimate'),
      findsOneWidget,
    );
  });

  testWidgets('the engine\'s refusal on an accrual reaches the person', (
    tester,
  ) async {
    final accruals = _FakeAccruals(
      refusal: 'The month 2026-11 is closed, so nothing can be posted into it.',
    );
    await _openAccruals(tester, accruals: accruals);

    await tester.tap(find.text('Choose the cost'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('6100 - Electricity').last);
    await tester.pumpAndSettle();
    await _fill(tester, 'What the month used', '10.00');
    await tester.tap(find.text('Charge the month and owe it'));
    await tester.pumpAndSettle();

    expect(
      find.text(
        'The month 2026-11 is closed, so nothing can be posted into it.',
      ),
      findsOneWidget,
    );
  });

  testWidgets(
    'a prepayment names the money that left and the account it sits in',
    (tester) async {
      final accruals = _FakeAccruals();
      await _openPrepayments(tester, accruals: accruals);

      await _fill(tester, 'Your reference', 'INS-1');
      await _fill(tester, 'What was paid', '1200.00');
      await tester.tap(find.text('Choose the cost'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('6300 - Insurance').last);
      await tester.pumpAndSettle();
      await tester.tap(find.text('The account the money sits in'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('1010 · Bank').last);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Record what was paid'));
      await tester.pumpAndSettle();

      expect(accruals.recorded.length, 1);
      final sent = accruals.recorded.first;
      expect(sent['reference'], 'INS-1');
      expect(sent['total'], '1200.00');
      expect(sent['months'], 12);
      expect(sent['paid_from'], 'BANK');
      expect(sent['money_account'], '1010');
      expect(sent['account'], '6300');
    },
  );

  testWidgets(
    'a prepayment offers the earliest month it has not used, and says what is left',
    (tester) async {
      final accruals = _FakeAccruals(
        paid: [
          _prepayment(
            id: 'p1',
            reference: 'INS-1',
            total: '1200.00',
            months: 12,
            released: const ['2026-01'],
            stillToUse: '1100.00',
          ),
        ],
      );
      await _openPrepayments(tester, accruals: accruals);

      expect(
        find.textContaining('1100.00 still to be used of 1200.00 paid'),
        findsOneWidget,
      );
      // January is already used up, so February is the one offered, with its own figure
      await tester.tap(find.text('Use up 2026-02 · 100.00'));
      await tester.pumpAndSettle();

      expect(accruals.usedUp, ['2026-02']);
    },
  );

  testWidgets('a prepayment with every month used up offers nothing further', (
    tester,
  ) async {
    final accruals = _FakeAccruals(
      paid: [
        _prepayment(
          id: 'p1',
          reference: 'INS-1',
          total: '200.00',
          months: 2,
          released: const ['2026-01', '2026-02'],
          stillToUse: '0.00',
        ),
      ],
    );
    await _openPrepayments(tester, accruals: accruals);

    expect(find.textContaining('Use up'), findsNothing);
    expect(find.text('Every month of it has been used up.'), findsOneWidget);
  });
}
