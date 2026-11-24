// A bill a supplier raised in another money.
//
// The one mistake this screen must never make is posting a dollar bill as though the dollars were the Book's own money. So
// what is pinned here is that the money is asked for, that a foreign bill goes to the engine with the supplier's own figure
// and the stated rate, and that no line is built on the device for it.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/banking/data/currencies.dart';
import 'package:vimbai_mobile_client/features/payables/data/supplier_account.dart';
import 'package:vimbai_mobile_client/features/payables/data/vendor_bill_posting.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/supplier_account_page.dart';

/// A supplier who bills in dollars.
class _FakeAccount extends SupplierAccountService {
  _FakeAccount(this.answer) : super.fresh();

  final SupplierAccount answer;

  @override
  Future<SupplierAccount> account(String supplierId) async => answer;
}

/// A Book that keeps dollars beside its own money.
class _FakeCurrencies extends CurrencyService {
  _FakeCurrencies() : super.fresh();

  @override
  Future<CurrencyBook> read() async => const CurrencyBook([
    IsoCurrency(code: 'USD', name: 'US Dollar', decimalPlaces: 2),
  ]);
}

/// Stands in for the engine's vendor bills route: it remembers what it was asked to post.
class _FakeBills extends VendorBillPostingService {
  _FakeBills() : super.fresh();

  final List<Map<String, Object>> posted = [];

  @override
  Future<VendorBillPosted> post({
    required String entryDate,
    required String billNumber,
    required String vendorId,
    required ExactAmount total,
    required String currency,
    required String rate,
    String dueDate = '',
  }) async {
    posted.add({
      'bill_number': billNumber,
      'total': total.toDecimalString(),
      'currency': currency,
      'rate': rate,
    });
    return VendorBillPosted(
      billNumber: billNumber,
      totalAmount: '25500.00',
      journalEntryId: 'je-1',
    );
  }
}

/// A ledger that would record a line if the page ever built one for this bill.
class _WatchingLedger extends AccountingApiService {
  final List<JournalEntry> written = [];

  @override
  Future<List<Account>> getAllAccounts() async => [
    Account(
      id: 'a2',
      accountNumber: '2000',
      accountName: 'Accounts Payable',
      accountType: 'liability',
      normalBalance: 'credit',
    ),
    Account(
      id: 'a6',
      accountNumber: '6000',
      accountName: 'Cost of Goods',
      accountType: 'expense',
      normalBalance: 'debit',
    ),
    Account(
      id: 'a3',
      accountNumber: '2100',
      accountName: 'VAT Payable',
      accountType: 'liability',
      normalBalance: 'credit',
    ),
  ];

  @override
  Future<JournalEntry> createJournalEntry(JournalEntry entry) async {
    written.add(entry);
    return entry;
  }
}

SupplierAccount _abroad() => SupplierAccount(
  supplierId: 'chinaco',
  name: 'China Co',
  lines: [
    SupplierAccountLine(
      date: '2026-11-18',
      kind: 'purchase_invoice',
      reference: 'CI-1',
      moves: true,
      amount: ExactAmount.parse('1000.00'),
      running: ExactAmount.parse('1000.00'),
    ),
  ],
  billed: ExactAmount.parse('1000.00'),
  paid: ExactAmount.zero,
  credited: ExactAmount.zero,
  balance: ExactAmount.parse('1000.00'),
  lastActivity: '2026-11-18',
);

void main() {
  testWidgets('a dollar bill is asked for its money and posted by the engine', (
    tester,
  ) async {
    final bills = _FakeBills();
    final ledger = _WatchingLedger();

    await tester.pumpWidget(
      MaterialApp(
        home: SupplierAccountPage(
          supplierId: 'chinaco',
          supplierName: 'China Co',
          service: _FakeAccount(_abroad()),
          ledger: ledger,
          currencies: _FakeCurrencies(),
          bills: bills,
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Bill received'));
    await tester.pumpAndSettle();

    // the money is asked for before any line is proposed
    expect(find.text('Which money is this bill in?'), findsOneWidget);
    await tester.tap(find.text('This Book\'s own money'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('USD · US Dollar').last);
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'Rate on the day of the bill'),
      '25.5',
    );
    await tester.tap(find.text('Next'));
    // the answer arrives a frame later, and a snackbar is only on screen while it is fresh
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));

    final sent = bills.posted.single;
    expect(sent['currency'], 'USD');
    expect(sent['rate'], '25.5');
    expect(
      sent['total'],
      '1000.00',
      reason: 'the supplier\'s own figure, not a converted one',
    );
    expect(
      ledger.written,
      isEmpty,
      reason: 'no line is built here, so dollars cannot land as Book money',
    );
    // and the bill is recorded once, against the supplier, with no second entry built anywhere
    expect(bills.posted.length, 1);
    expect(sent['bill_number'], 'CI-1');
  });
}
