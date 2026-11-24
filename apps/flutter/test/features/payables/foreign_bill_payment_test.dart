// Paying a bill the supplier raised in another money.
//
// Two things must both be true or the books stop matching the bank: the payment is recorded in the money it was billed in, at
// the rate of the day, and the difference between what the books carried and what actually left is posted on the payable
// side. A payment that records only the first leaves a payable standing at a figure nobody paid.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/payables/data/foreign_bill_payment.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/pay_supplier_out_sheet.dart';

/// Stands in for the bills route: one bill, billed in dollars, 600 still owed.
class _FakeBills extends ForeignBillPaymentService {
  _FakeBills() : super.fresh();

  final List<Map<String, Object>> differences = [];

  @override
  Future<BillInAnotherMoney?> find({
    required String vendorId,
    required String reference,
  }) async => BillInAnotherMoney(
    billNumber: reference,
    currency: 'USD',
    rate: ExactAmount.parse('25.5'),
    outstandingUnits: ExactAmount.parse('600.00'),
    outstandingCarried: ExactAmount.parse('15300.00'),
  );

  @override
  Future<void> recordDifference({
    required String entryDate,
    required String reference,
    required ExactAmount unitsPaid,
    required ExactAmount unitsTotal,
    required ExactAmount carriedTotal,
    required ExactAmount rate,
  }) async {
    differences.add({
      'reference': reference,
      'units_paid': unitsPaid.toDecimalString(),
      'units_total': unitsTotal.toDecimalString(),
      'carried_total': carriedTotal.toDecimalString(),
      'rate': rate.toDecimalString(),
      'entry_date': entryDate,
    });
  }
}

/// A cash book that records what it was asked to, without a server.
class _FakeCash extends CashBookService {
  _FakeCash();

  final List<Map<String, Object>> written = [];

  @override
  Future<List<CashAccount>> accounts() async => [
    CashAccount(
      id: 'ca-1',
      code: '1000',
      name: 'Bank',
      type: 'bank',
      bankName: '',
      currency: 'USD',
      openingBalance: ExactAmount.zero,
      balance: ExactAmount.parse('100000.00'),
    ),
  ];

  @override
  Future<CashEntry> record({
    required CashAccount account,
    required bool moneyIn,
    required ExactAmount amount,
    required String description,
    required String date,
    String voucher = '',
    String againstAccount = '',
    String currency = '',
    String rate = '',
  }) async {
    written.add({
      'amount': amount.toDecimalString(),
      'currency': currency,
      'rate': rate,
      'voucher': voucher,
      'against': againstAccount,
    });
    return CashEntry.fromJson({
      'id': 'ce-1',
      'entry_date': date,
      'description': description,
      'amount': amount.toDecimalString(),
      'is_debit': false,
      'bank_account': account.code,
    });
  }
}

class _FakeLedger extends AccountingApiService {
  @override
  Future<List<Account>> getAllAccounts() async => [
    Account(
      id: 'a2',
      accountNumber: '2000',
      accountName: 'Accounts Payable',
      accountType: 'liability',
      normalBalance: 'credit',
    ),
  ];
}

void main() {
  testWidgets(
    'a dollar bill is paid in dollars at the rate of the day, and the difference is posted',
    (tester) async {
      final cash = _FakeCash();
      final bills = _FakeBills();

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: PaySupplierOutSheet(
              amount: ExactAmount.parse('15300.00'),
              supplierName: 'China Co',
              cash: cash,
              ledger: _FakeLedger(),
              reference: 'BILL-1',
              vendorId: 'chinaco',
              bills: bills,
              onDate: '2026-11-20',
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // the bill's own money and what is still owed are shown, not the Book's figure alone
      expect(find.textContaining('600.00 USD still owed'), findsOneWidget);

      await tester.enterText(
        find.widgetWithText(TextField, 'What one USD is worth today'),
        '27',
      );
      await tester.pumpAndSettle();
      expect(find.textContaining('16200.00'), findsWidgets);

      await tester.tap(find.text('Record it'));
      await tester.pumpAndSettle();

      // the movement is recorded in dollars, at the rate of the day, against the bill
      final entry = cash.written.single;
      expect(entry['amount'], '600.00', reason: 'what the supplier billed in');
      expect(entry['currency'], 'USD');
      expect(entry['rate'], '27.00');
      expect(entry['voucher'], 'BILL-1');

      // and the rate difference is posted for the share that was paid, worked out by the engine
      final difference = bills.differences.single;
      expect(difference['units_paid'], '600.00');
      expect(difference['units_total'], '600.00');
      expect(difference['carried_total'], '15300.00');
      expect(difference['rate'], '27.00');
      expect(difference['entry_date'], '2026-11-20');
    },
  );

  testWidgets(
    'paying part of a dollar bill records the part and asks for its share',
    (tester) async {
      final cash = _FakeCash();
      final bills = _FakeBills();

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: PaySupplierOutSheet(
              amount: ExactAmount.parse('15300.00'),
              supplierName: 'China Co',
              cash: cash,
              ledger: _FakeLedger(),
              reference: 'BILL-1',
              vendorId: 'chinaco',
              bills: bills,
              onDate: '2026-11-20',
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // all of it is offered until the person says less
      expect(find.text('600.00'), findsWidgets);
      await tester.enterText(
        find.widgetWithText(TextField, 'How much USD is being paid'),
        '200',
      );
      await tester.enterText(
        find.widgetWithText(TextField, 'What one USD is worth today'),
        '27',
      );
      await tester.pumpAndSettle();
      expect(find.textContaining('5400.00'), findsWidgets);

      await tester.tap(find.text('Record it'));
      await tester.pumpAndSettle();

      expect(cash.written.single['amount'], '200.00');
      final difference = bills.differences.single;
      expect(difference['units_paid'], '200.00');
      expect(difference['units_total'], '600.00');
      expect(difference['carried_total'], '15300.00');
    },
  );

  testWidgets(
    'more than the bill still owes is refused before anything is written',
    (tester) async {
      final cash = _FakeCash();
      final bills = _FakeBills();

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: PaySupplierOutSheet(
              amount: ExactAmount.parse('15300.00'),
              supplierName: 'China Co',
              cash: cash,
              ledger: _FakeLedger(),
              reference: 'BILL-1',
              vendorId: 'chinaco',
              bills: bills,
              onDate: '2026-11-20',
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.enterText(
        find.widgetWithText(TextField, 'How much USD is being paid'),
        '700',
      );
      await tester.enterText(
        find.widgetWithText(TextField, 'What one USD is worth today'),
        '27',
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Record it'));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('Only 600.00 USD is still owed'),
        findsOneWidget,
      );
      expect(
        cash.written,
        isEmpty,
        reason: 'nothing is recorded when it cannot be',
      );
      expect(bills.differences, isEmpty);
    },
  );

  testWidgets('a bill in the Book\'s own money is paid exactly as before', (
    tester,
  ) async {
    final cash = _FakeCash();

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: PaySupplierOutSheet(
            amount: ExactAmount.parse('450.00'),
            supplierName: 'Paper Co',
            cash: cash,
            ledger: _FakeLedger(),
            reference: 'INV-77',
            vendorId: 'paperco',
            bills: _NoForeignBill(),
            onDate: '2026-11-20',
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('What one'), findsNothing);
    await tester.tap(find.text('Record it'));
    await tester.pumpAndSettle();

    final entry = cash.written.single;
    expect(entry['amount'], '450.00');
    expect(entry['currency'], '');
    expect(entry['rate'], '');
  });
}

/// A Book whose bills are all in its own money.
class _NoForeignBill extends ForeignBillPaymentService {
  _NoForeignBill() : super.fresh();

  @override
  Future<BillInAnotherMoney?> find({
    required String vendorId,
    required String reference,
  }) async => null;
}
