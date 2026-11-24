// Recording a general expense.
//
// A cost paid and a cost incurred are not the same event: one moves money the business holds, the other creates a liability.
// What this pins is that the plain answer to "has it been paid?" decides which, that the tax inside the figure is sent as
// inside it rather than added to it, and that a payment always names both the account the money left and the account it sits
// in, because the cash book and the ledger are two records of one movement.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/banking/data/currencies.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/expenses/data/expense_posting.dart';
import 'package:vimbai_mobile_client/features/expenses/presentation/record_expense_page.dart';

/// The Book's chart: where costs land, where money sits, and the account tax is owed to.
class _FakeLedger extends AccountingApiService {
  _FakeLedger({this.twoTaxAccounts = false});

  final bool twoTaxAccounts;

  @override
  Future<List<Account>> getAllAccounts() async => [
    Account(
      accountNumber: '1010',
      accountName: 'Bank',
      accountType: 'asset',
      normalBalance: 'debit',
    ),
    Account(
      accountNumber: '2100',
      accountName: 'VAT Payable',
      accountType: 'liability',
      normalBalance: 'credit',
    ),
    if (twoTaxAccounts)
      Account(
        accountNumber: '2110',
        accountName: 'Withholding Tax Payable',
        accountType: 'liability',
        normalBalance: 'credit',
      ),
    Account(
      accountNumber: '6100',
      accountName: 'Rent',
      accountType: 'expense',
      normalBalance: 'debit',
    ),
    Account(
      accountNumber: '6300',
      accountName: 'Office Expenses',
      accountType: 'expense',
      normalBalance: 'debit',
    ),
  ];
}

/// The money the business holds.
class _FakeCashBook extends CashBookService {
  @override
  Future<List<CashAccount>> accounts() async => const [
    CashAccount(
      id: 'acc-1',
      code: 'BANK-1',
      name: 'CBZ Current',
      type: 'bank',
      bankName: 'CBZ',
      currency: 'USD',
      openingBalance: ExactAmount.zero,
      balance: ExactAmount.zero,
    ),
  ];
}

class _FakeCurrencies extends CurrencyService {
  _FakeCurrencies() : super.fresh();

  @override
  Future<CurrencyBook> read() async => const CurrencyBook([
    IsoCurrency(code: 'USD', name: 'US Dollar', decimalPlaces: 2),
  ]);
}

/// Stands in for the engine's expenses journal: it remembers what it was asked to record.
class _FakeExpenses extends ExpensePostingService {
  _FakeExpenses() : super.fresh();

  final List<Map<String, Object?>> posted = [];

  @override
  Future<ExpenseRecorded> post({
    required String entryDate,
    required String reference,
    required String expenseAccount,
    required ExactAmount total,
    String paidTo = '',
    ExactAmount? tax,
    String taxAccount = '',
    String paidFrom = '',
    String moneyAccount = '',
    String description = '',
    String currency = '',
    String rate = '',
    String notes = '',
  }) async {
    posted.add({
      'reference': reference,
      'account': expenseAccount,
      'total': total.toDecimalString(),
      'paid_to': paidTo,
      'tax': tax?.toDecimalString(),
      'tax_account': taxAccount,
      'paid_from': paidFrom,
      'money_account': moneyAccount,
      'currency': currency,
      'rate': rate,
    });
    return ExpenseRecorded(
      reference: reference,
      totalAmount: '1500.00',
      journalEntryId: 'entry-1',
      cashEntryId: paidFrom.isEmpty ? '' : 'cash-1',
    );
  }
}

Future<void> _fill(WidgetTester tester, String label, String text) async {
  await tester.enterText(find.widgetWithText(TextField, label), text);
  await tester.pumpAndSettle();
}

Future<void> _choose(WidgetTester tester, String label, String option) async {
  await tester.tap(find.text(label));
  await tester.pumpAndSettle();
  await tester.tap(find.text(option).last);
  await tester.pumpAndSettle();
}

Future<void> _record(WidgetTester tester) async {
  await tester.tap(find.text('Record the expense'));
  await tester.pumpAndSettle();
}

Future<void> _open(
  WidgetTester tester, {
  required _FakeExpenses expenses,
  bool twoTaxAccounts = false,
}) async {
  // the form is taller than a phone, so it is given a tall surface: every field is on screen and nothing is hidden
  tester.view.physicalSize = const Size(1000, 3000);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      home: RecordExpensePage(
        posting: expenses,
        ledger: _FakeLedger(twoTaxAccounts: twoTaxAccounts),
        cashbook: _FakeCashBook(),
        currencies: _FakeCurrencies(),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('an expense that is owed names no money account at all', (
    tester,
  ) async {
    final expenses = _FakeExpenses();
    await _open(tester, expenses: expenses);

    await _choose(tester, 'What the cost was', '6100 · Rent');
    await _fill(tester, 'Your reference for this expense', 'EXP-1');
    await _fill(tester, 'What was spent', '1500');
    await _record(tester);

    final posted = expenses.posted.single;
    expect(posted['account'], '6100');
    expect(posted['total'], '1500.00');
    expect(posted['paid_from'], '', reason: 'nothing has been paid yet');
    expect(posted['money_account'], '');
  });

  testWidgets(
    'an expense that was paid names the account it left and the account it sits in',
    (tester) async {
      final expenses = _FakeExpenses();
      await _open(tester, expenses: expenses);

      await _choose(tester, 'What the cost was', '6300 · Office Expenses');
      await _fill(tester, 'Your reference for this expense', 'EXP-2');
      await _fill(tester, 'What was spent', '300');
      await _choose(tester, 'Has it been paid?', 'Paid from CBZ Current');
      await _choose(tester, 'The account that money sits in', '1010 · Bank');
      await _record(tester);

      final posted = expenses.posted.single;
      expect(posted['paid_from'], 'BANK-1');
      expect(posted['money_account'], '1010');
    },
  );

  testWidgets(
    'the tax is sent as inside the figure, on the account that holds it',
    (tester) async {
      final expenses = _FakeExpenses();
      await _open(tester, expenses: expenses);

      await _choose(tester, 'What the cost was', '6300 · Office Expenses');
      await _fill(tester, 'Your reference for this expense', 'EXP-3');
      await _fill(tester, 'What was spent', '1150');
      await _fill(tester, 'Tax inside that figure (if any)', '150');
      await _record(tester);

      final posted = expenses.posted.single;
      expect(
        posted['total'],
        '1150.00',
        reason: 'the gross figure as it stands',
      );
      expect(posted['tax'], '150.00');
      expect(
        posted['tax_account'],
        '2100',
        reason:
            'the Book has one tax account, so it is proposed rather than asked for',
      );
    },
  );

  testWidgets(
    'a cost in another money carries its money and rate, with nothing converted here',
    (tester) async {
      final expenses = _FakeExpenses();
      await _open(tester, expenses: expenses);

      await _choose(tester, 'What the cost was', '6100 · Rent');
      await _fill(tester, 'Your reference for this expense', 'EXP-4');
      await _fill(tester, 'What was spent', '100');
      await _choose(tester, 'Which money it was spent in', 'USD · US Dollar');
      await _fill(tester, 'Rate on the day it was spent', '25.5');
      await _record(tester);

      final posted = expenses.posted.single;
      expect(posted['total'], '100.00', reason: 'what the receipt showed');
      expect(posted['currency'], 'USD');
      expect(posted['rate'], '25.5');
    },
  );

  testWidgets(
    'a cost with no account, no reference or nothing spent is refused, naming what is missing',
    (tester) async {
      final expenses = _FakeExpenses();
      await _open(tester, expenses: expenses);

      await _fill(tester, 'What was spent', '500');
      await _record(tester);
      expect(
        find.textContaining('which account the cost belongs in'),
        findsOneWidget,
      );

      await _choose(tester, 'What the cost was', '6100 · Rent');
      await _record(tester);
      expect(find.textContaining('reference of your own'), findsOneWidget);

      await _fill(tester, 'Your reference for this expense', 'EXP-5');
      await _fill(tester, 'What was spent', '0');
      await _record(tester);
      expect(find.textContaining('what was actually spent'), findsOneWidget);
      expect(expenses.posted, isEmpty);
    },
  );

  testWidgets(
    'a payment with no chart account named is refused before it is sent',
    (tester) async {
      final expenses = _FakeExpenses();
      await _open(tester, expenses: expenses);

      await _choose(tester, 'What the cost was', '6100 · Rent');
      await _fill(tester, 'Your reference for this expense', 'EXP-6');
      await _fill(tester, 'What was spent', '500');
      await _choose(tester, 'Has it been paid?', 'Paid from CBZ Current');
      await _record(tester);

      expect(
        find.textContaining('which account in the chart the money sits in'),
        findsOneWidget,
      );
      expect(expenses.posted, isEmpty);
    },
  );

  testWidgets(
    'tax bigger than what was spent is refused rather than reclaiming more than was charged',
    (tester) async {
      final expenses = _FakeExpenses();
      await _open(tester, expenses: expenses);

      await _choose(tester, 'What the cost was', '6300 · Office Expenses');
      await _fill(tester, 'Your reference for this expense', 'EXP-7');
      await _fill(tester, 'What was spent', '100');
      await _fill(tester, 'Tax inside that figure (if any)', '150');
      await _record(tester);

      expect(
        find.textContaining('cannot be bigger than what was spent'),
        findsOneWidget,
      );
      expect(expenses.posted, isEmpty);
    },
  );

  testWidgets(
    'a Book with two tax accounts asks which one rather than guessing',
    (tester) async {
      final expenses = _FakeExpenses();
      await _open(tester, expenses: expenses, twoTaxAccounts: true);

      await _choose(tester, 'What the cost was', '6300 · Office Expenses');
      await _fill(tester, 'Your reference for this expense', 'EXP-8');
      await _fill(tester, 'What was spent', '1150');
      await _fill(tester, 'Tax inside that figure (if any)', '150');
      await _record(tester);
      expect(
        find.textContaining('more than one account for tax'),
        findsOneWidget,
      );

      await _choose(
        tester,
        'Which tax account',
        '2110 · Withholding Tax Payable',
      );
      await _record(tester);
      expect(expenses.posted.single['tax_account'], '2110');
    },
  );
}
