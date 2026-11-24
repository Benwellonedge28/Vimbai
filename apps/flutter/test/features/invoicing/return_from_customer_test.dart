// A customer sending goods back.
//
// A return is the sale turned round: the customer owes less, the sale is taken back, and no money goes out. What this pins is
// that the gross credit reaches the engine as the credit note states it, that tax comes out of that gross rather than being
// added to it, that a return taken against an invoice carries that invoice's money and rate, and that a credit bigger than
// the invoice is refused rather than leaving the customer owing less than nothing.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/banking/data/currencies.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/sales_return_posting.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/return_from_customer_page.dart';

/// The customers and invoices the engine already holds.
class _FakeInvoicing extends InvoicingService {
  _FakeInvoicing() : super.fresh();

  @override
  Future<List<InvoiceCustomer>> listCustomers() async => const [
    InvoiceCustomer(id: 'c1', customerId: 'acme', name: 'Acme Ltd'),
  ];

  @override
  Future<List<Invoice>> listInvoices() async => [
    Invoice(
      invoiceNumber: 'INV-7',
      customerId: 'acme',
      invoiceDate: '2026-11-02',
      dueDate: '2026-12-02',
      total: ExactAmount.parse('4000'),
      currency: 'USD',
      rate: '25.5',
      paid: ExactAmount.zero,
      status: 'unpaid',
      lines: const [],
    ),
    Invoice(
      invoiceNumber: 'INV-8',
      customerId: 'acme',
      invoiceDate: '2026-11-09',
      dueDate: '2026-12-09',
      total: ExactAmount.parse('1150'),
      paid: ExactAmount.zero,
      status: 'unpaid',
      lines: const [],
    ),
  ];
}

/// A Book that keeps dollars beside its own money.
class _FakeCurrencies extends CurrencyService {
  _FakeCurrencies() : super.fresh();

  @override
  Future<CurrencyBook> read() async => const CurrencyBook([
    IsoCurrency(code: 'USD', name: 'US Dollar', decimalPlaces: 2),
  ]);
}

/// Stands in for the engine's sales returns journal: it remembers what it was asked to post.
class _FakeReturns extends SalesReturnPostingService {
  _FakeReturns() : super.fresh();

  final List<Map<String, Object?>> posted = [];

  @override
  Future<SalesReturnPosted> post({
    required String entryDate,
    required String returnNumber,
    required String customerId,
    required ExactAmount total,
    ExactAmount? tax,
    String originalInvoice = '',
    String reason = '',
    String currency = '',
    String rate = '',
    String notes = '',
  }) async {
    posted.add({
      'entry_date': entryDate,
      'reference': returnNumber,
      'customer': customerId,
      'total': total.toDecimalString(),
      'tax': tax?.toDecimalString(),
      'original_invoice': originalInvoice,
      'reason': reason,
      'currency': currency,
      'rate': rate,
    });
    return SalesReturnPosted(
      returnNumber: returnNumber,
      totalAmount: '12750.00',
      journalEntryId: 'entry-1',
    );
  }
}

Future<void> _fill(WidgetTester tester, String label, String text) async {
  await tester.enterText(find.widgetWithText(TextField, label), text);
  await tester.pumpAndSettle();
}

Future<void> _record(WidgetTester tester) async {
  await tester.tap(find.text('Record the return'));
  await tester.pumpAndSettle();
}

Future<void> _open(
  WidgetTester tester, {
  required _FakeReturns returns,
  String customerId = '',
}) async {
  // the form is taller than a phone, so it is given a tall surface: every field is on screen and nothing is hidden
  tester.view.physicalSize = const Size(1000, 2800);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      home: ReturnFromCustomerPage(
        posting: returns,
        invoicing: _FakeInvoicing(),
        currencies: _FakeCurrencies(),
        customerId: customerId,
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'a return in the Book\'s own money sends no currency and no rate',
    (tester) async {
      final returns = _FakeReturns();
      await _open(tester, returns: returns, customerId: 'acme');

      await _fill(tester, 'Your reference for this return', 'CN-1');
      await _fill(tester, 'What is being credited back', '1150');
      await _record(tester);

      final posted = returns.posted.single;
      expect(posted['reference'], 'CN-1');
      expect(posted['customer'], 'acme');
      expect(posted['total'], '1150.00');
      expect(
        posted['currency'],
        '',
        reason: 'the Book\'s own money names nothing',
      );
      expect(posted['rate'], '');
    },
  );

  testWidgets('the tax is carried inside the credit, not added to it', (
    tester,
  ) async {
    final returns = _FakeReturns();
    await _open(tester, returns: returns, customerId: 'acme');

    await _fill(tester, 'Your reference for this return', 'CN-2');
    await _fill(tester, 'What is being credited back', '1150');
    await _fill(tester, 'Tax inside that figure (if any)', '150');
    await _record(tester);

    final posted = returns.posted.single;
    expect(posted['total'], '1150.00', reason: 'the gross credit as it stands');
    expect(posted['tax'], '150.00');
  });

  testWidgets(
    'a return taken against a dollar invoice carries that invoice\'s money and rate',
    (tester) async {
      final returns = _FakeReturns();
      await _open(tester, returns: returns, customerId: 'acme');

      await _fill(tester, 'Your reference for this return', 'CN-3');
      await tester.tap(find.text('The invoice it reverses (if known)'));
      await tester.pumpAndSettle();
      await tester.tap(find.textContaining('INV-7').last);
      await tester.pumpAndSettle();
      await _fill(tester, 'What is being credited back', '1000');
      await _record(tester);

      final posted = returns.posted.single;
      expect(posted['original_invoice'], 'INV-7');
      expect(
        posted['currency'],
        'USD',
        reason: 'the invoice was raised in dollars',
      );
      expect(
        posted['rate'],
        '25.5',
        reason: 'the rate the sale was carried at',
      );
      expect(posted['total'], '1000.00', reason: 'what the customer was shown');
    },
  );

  testWidgets(
    'a credit bigger than the invoice is refused rather than leaving them owing less than nothing',
    (tester) async {
      final returns = _FakeReturns();
      await _open(tester, returns: returns, customerId: 'acme');

      await _fill(tester, 'Your reference for this return', 'CN-4');
      await tester.tap(find.text('The invoice it reverses (if known)'));
      await tester.pumpAndSettle();
      await tester.tap(find.textContaining('INV-8').last);
      await tester.pumpAndSettle();
      await _fill(tester, 'What is being credited back', '2000');
      await _record(tester);

      expect(
        find.textContaining('more than INV-8 was billed at'),
        findsOneWidget,
      );
      expect(returns.posted, isEmpty);
    },
  );

  testWidgets(
    'a return with no customer or no reference is refused, naming what is missing',
    (tester) async {
      final returns = _FakeReturns();
      await _open(tester, returns: returns);

      await _fill(tester, 'What is being credited back', '500');
      await _record(tester);
      expect(find.textContaining('which customer'), findsOneWidget);

      await _fill(tester, 'Customer', 'acme');
      await _record(tester);
      expect(find.textContaining('reference of your own'), findsOneWidget);
      expect(returns.posted, isEmpty);
    },
  );

  testWidgets(
    'tax bigger than the credit is refused rather than reversing more tax than was charged',
    (tester) async {
      final returns = _FakeReturns();
      await _open(tester, returns: returns, customerId: 'acme');

      await _fill(tester, 'Your reference for this return', 'CN-5');
      await _fill(tester, 'What is being credited back', '100');
      await _fill(tester, 'Tax inside that figure (if any)', '150');
      await _record(tester);

      expect(
        find.textContaining('cannot be bigger than the credit'),
        findsOneWidget,
      );
      expect(returns.posted, isEmpty);
    },
  );

  testWidgets(
    'a credit in another money without a rate is refused before it is sent',
    (tester) async {
      final returns = _FakeReturns();
      await _open(tester, returns: returns, customerId: 'acme');

      await _fill(tester, 'Your reference for this return', 'CN-6');
      await _fill(tester, 'What is being credited back', '500');
      await tester.tap(find.text('This Book\'s own money'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('USD · US Dollar').last);
      await tester.pumpAndSettle();
      await _record(tester);

      expect(find.textContaining('Say what one USD is worth'), findsOneWidget);
      expect(returns.posted, isEmpty);
    },
  );
}
