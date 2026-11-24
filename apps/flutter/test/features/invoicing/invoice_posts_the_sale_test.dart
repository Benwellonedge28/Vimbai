// The sale itself reaching the ledger.
//
// Recording a payment is bookkeeping against the invoice; the money reaching the books is a journal entry, and the ledger
// has one route. What is tested here is that the phone walks that route honestly: it proposes the two accounts from the
// person's own chart, writes nothing until they agree, refuses rather than guesses when the chart has no account to name,
// and carries the invoice number as the reference so a second attempt cannot post the same sale twice.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/sale_posting.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/invoice_detail_page.dart';
import 'package:vimbai_mobile_client/features/tax/data/sale_tax.dart';
import 'package:vimbai_mobile_client/features/tax/data/tax_table.dart';

Invoice _discounted() => Invoice(
  invoiceNumber: 'INV-0002',
  customerId: 'acme',
  invoiceDate: '2026-11-18',
  dueDate: '2026-12-18',
  total: ExactAmount.parse('100000.00'),
  discount: ExactAmount.parse('5000.00'),
  amountDue: ExactAmount.parse('95000.00'),
  paid: ExactAmount.zero,
  status: 'outstanding',
  lines: [
    InvoiceLine(
      description: 'Car',
      quantity: ExactAmount.parse('1'),
      unitPrice: ExactAmount.parse('100000.00'),
    ),
  ],
);

Invoice _invoice() => Invoice(
  invoiceNumber: 'INV-0001',
  customerId: 'acme',
  invoiceDate: '2026-11-18',
  dueDate: '2026-12-18',
  total: ExactAmount.parse('779.985'),
  paid: ExactAmount.zero,
  status: 'outstanding',
  lines: [
    InvoiceLine(
      description: 'Consulting',
      quantity: ExactAmount.parse('3'),
      unitPrice: ExactAmount.parse('250.00'),
    ),
  ],
);

Account _account(String number, String name, String type) => Account(
  accountNumber: number,
  accountName: name,
  accountType: type,
  normalBalance: type == 'asset' ? 'debit' : 'credit',
);

/// Stands in for the ledger: it answers a chart, and remembers what it was asked to write.
class _FakeLedger extends AccountingApiService {
  _FakeLedger(this.chart);

  final List<Account> chart;
  final List<JournalEntry> written = [];

  @override
  Future<List<Account>> getAllAccounts() async => chart;

  @override
  Future<JournalEntry> createJournalEntry(JournalEntry entry) async {
    written.add(entry);
    return entry;
  }
}

/// Stands in for the engine's sales journal: it remembers what it was asked to post.
class _FakeSales extends SalePostingService {
  _FakeSales() : super.fresh();

  final List<Map<String, Object>> posted = [];

  @override
  Future<SalePosted> post({
    required String entryDate,
    required String invoiceNumber,
    required String customerId,
    required ExactAmount total,
    required List<String> taxes,
    ExactAmount? discount,
    String currency = '',
    String rate = '',
    String status = 'outstanding',
  }) async {
    posted.add({
      'invoice_number': invoiceNumber,
      'total': total.toDecimalString(),
      'currency': currency,
      'rate': rate,
      'taxes': taxes,
    });
    return SalePosted(
      invoiceNumber: invoiceNumber,
      totalAmount: '2550000.00',
      taxAmount: '0',
      journalEntryId: 'je-usd',
    );
  }
}

/// A Book that charges no tax, so the plain sale can be exercised on its own.
class _NoTax implements SaleTaxReads {
  const _NoTax();

  @override
  Future<List<BookTax>> chargedOnSales(String date) async => const [];

  @override
  Future<TaxPreview> preview({
    required String date,
    required String gross,
    required List<String> taxes,
    String discount = '0',
  }) async => throw UnimplementedError('nothing was chosen');
}

/// A Book that charges VAT at 15.5%, answering with the engine's own figures for a 100,000.00 sale.
class _VatBook implements SaleTaxReads {
  _VatBook(this.gross);

  final ExactAmount gross;
  final List<String> asked = [];

  /// What the page told the engine was taken off the invoice, so a test can pin that the tax followed the discounted
  /// figure rather than the gross one.
  String askedDiscount = '';

  @override
  Future<List<BookTax>> chargedOnSales(String date) async => [
    BookTax(
      id: 'vat',
      name: 'VAT',
      rate: ExactAmount.parse('15.5'),
      scope: 'sales',
      collectedAccount: '2100',
      reclaimAccount: '2100',
      appliesAfter: const [],
      effectiveFrom: '2026-01-01',
      effectiveTo: '',
      inForce: true,
    ),
  ];

  @override
  Future<TaxPreview> preview({
    required String date,
    required String gross,
    required List<String> taxes,
    String discount = '0',
  }) async {
    asked.addAll(taxes);
    askedDiscount = discount;
    // this Book answers with the figure it was built for, which stands in for the engine taking the discount off first
    final total = this.gross;
    final tax = ExactAmount.parse('13419.91');
    final net = total - tax;
    return TaxPreview(
      net: net,
      tax: tax,
      gross: total,
      lines: [
        TaxLine(
          tax: 'VAT',
          rate: ExactAmount.parse('15.5'),
          base: net,
          amount: tax,
          side: 'credit',
          accountNumber: '2100',
          expenseAccount: '6900',
        ),
      ],
      note: '',
    );
  }
}

void main() {
  testWidgets(
    'a discounted sale posts what the customer owes, and the tax is asked for on that figure',
    (tester) async {
      final ledger = _FakeLedger([
        _account('1100', 'Accounts Receivable', 'asset'),
        _account('4000', 'Sales', 'revenue'),
      ]);
      final book = _VatBook(ExactAmount.parse('95000.00'));

      await tester.pumpWidget(
        MaterialApp(
          home: InvoiceDetailPage(
            invoice: _discounted(),
            accounting: ledger,
            tax: book,
          ),
        ),
      );
      await tester.pumpAndSettle();

      // the discount is on the invoice where a person can see it, and what is owed is the figure after it
      expect(find.textContaining('Less discount'), findsOneWidget);
      expect(find.textContaining('95000.00'), findsWidgets);

      await tester.tap(find.text('Post the sale to the ledger'));
      await tester.pumpAndSettle();

      // the tax is chosen, never typed, and the engine is asked what it comes to on the discounted figure
      await tester.tap(find.text('VAT 15.5%'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Post it'));
      await tester.pumpAndSettle();

      expect(
        book.askedDiscount,
        '5000.00',
        reason: 'the tax must follow the discounted figure, not the gross one',
      );
      final entry = ledger.written.single;
      expect(
        entry.lines.first.debit,
        ExactAmount.parse('95000.00'),
        reason:
            'the customer owes what is left after the discount, not the gross invoice',
      );
    },
  );

  testWidgets(
    'the sale is posted as a balanced entry, with the invoice number as its reference',
    (tester) async {
      final ledger = _FakeLedger([
        _account('1000', 'Cash', 'asset'),
        _account('1100', 'Accounts Receivable', 'asset'),
        _account('4000', 'Sales', 'revenue'),
      ]);

      await tester.pumpWidget(
        MaterialApp(
          home: InvoiceDetailPage(
            invoice: _invoice(),
            accounting: ledger,
            tax: const _NoTax(),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Post the sale to the ledger'));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('Dr 1100 Accounts Receivable'),
        findsOneWidget,
      );
      expect(find.textContaining('Cr 4000 Sales'), findsOneWidget);
      expect(
        ledger.written,
        isEmpty,
        reason: 'nothing may be written before the person agrees',
      );

      await tester.tap(find.text('Post it'));
      await tester.pumpAndSettle();

      expect(ledger.written.length, 1);
      final entry = ledger.written.single;
      expect(
        entry.referenceNumber,
        'INV-0001',
        reason: 'the reference is what stops a second posting',
      );
      expect(entry.status, 'posted');
      expect(entry.entryDate, DateTime(2026, 11, 18));
      expect(entry.lines.length, 2);
      expect(entry.lines[0].accountNumber, '1100');
      expect(entry.lines[0].debit, ExactAmount.parse('779.985'));
      expect(entry.lines[1].accountNumber, '4000');
      expect(entry.lines[1].credit, ExactAmount.parse('779.985'));
      expect(entry.lines[0].debit, entry.lines[1].credit);
    },
  );

  testWidgets(
    'a chart with nowhere to put the debt is refused, not guessed at',
    (tester) async {
      final ledger = _FakeLedger([
        _account('1000', 'Cash', 'asset'),
        _account('4000', 'Sales', 'revenue'),
      ]);

      await tester.pumpWidget(
        MaterialApp(
          home: InvoiceDetailPage(
            invoice: _invoice(),
            accounting: ledger,
            tax: const _NoTax(),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Post the sale to the ledger'));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('no Accounts Receivable account'),
        findsOneWidget,
      );
      expect(find.text('Post it'), findsNothing);
      expect(ledger.written, isEmpty);
    },
  );

  testWidgets(
    'a sale that carries tax posts the tax out of the total, to its own account',
    (tester) async {
      final ledger = _FakeLedger([
        _account('1100', 'Accounts Receivable', 'asset'),
        _account('4000', 'Sales', 'revenue'),
        _account('2100', 'VAT Payable', 'liability'),
      ]);
      final car = Invoice(
        invoiceNumber: 'CAR-1',
        customerId: 'acme',
        invoiceDate: '2026-11-19',
        dueDate: '2026-12-19',
        total: ExactAmount.parse('100000.00'),
        paid: ExactAmount.zero,
        status: 'outstanding',
        lines: const [],
      );
      final tax = _VatBook(ExactAmount.parse('100000.00'));

      await tester.pumpWidget(
        MaterialApp(
          home: InvoiceDetailPage(invoice: car, accounting: ledger, tax: tax),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Post the sale to the ledger'));
      await tester.pumpAndSettle();

      expect(find.text('VAT 15.5%'), findsOneWidget);
      await tester.tap(find.text('VAT 15.5%'));
      await tester.pumpAndSettle();

      expect(tax.asked, ['VAT'], reason: 'the tax is chosen, never typed');
      expect(
        find.textContaining('13419.91'),
        findsWidgets,
        reason: 'the engine own figure is shown before anything is posted',
      );
      expect(ledger.written, isEmpty);

      await tester.tap(find.text('Post it'));
      await tester.pumpAndSettle();

      final entry = ledger.written.single;
      expect(entry.lines.length, 4);
      expect(entry.lines[0].accountNumber, '1100');
      expect(entry.lines[0].debit, ExactAmount.parse('100000.00'));
      expect(
        entry.lines[1].accountNumber,
        '4000',
        reason: 'the sale records what the customer paid, in full',
      );
      expect(entry.lines[1].credit, ExactAmount.parse('100000.00'));
      expect(
        entry.lines[2].accountNumber,
        '6900',
        reason: 'the tax is its own expense, named by its type',
      );
      expect(entry.lines[2].debit, ExactAmount.parse('13419.91'));
      expect(entry.lines[3].accountNumber, '2100');
      expect(entry.lines[3].credit, ExactAmount.parse('13419.91'));
      final debits = entry.lines.fold(
        ExactAmount.zero,
        (sum, line) => sum + line.debit,
      );
      final credits = entry.lines.fold(
        ExactAmount.zero,
        (sum, line) => sum + line.credit,
      );
      expect(debits, credits, reason: 'the entry still balances');
    },
  );

  testWidgets(
    'a sale billed abroad is posted through the engine, at the rate it was raised at',
    (tester) async {
      final ledger = _FakeLedger([
        _account('1100', 'Accounts Receivable', 'asset'),
        _account('4000', 'Sales', 'revenue'),
      ]);
      final sales = _FakeSales();
      final abroad = Invoice(
        invoiceNumber: 'USD-1',
        customerId: 'acme',
        invoiceDate: '2026-11-18',
        dueDate: '2026-12-18',
        total: ExactAmount.parse('100000.00'),
        currency: 'USD',
        rate: '25.500000',
        paid: ExactAmount.zero,
        status: 'outstanding',
        lines: [
          InvoiceLine(
            description: 'Car',
            quantity: ExactAmount.parse('1'),
            unitPrice: ExactAmount.parse('100000.00'),
          ),
        ],
      );

      await tester.pumpWidget(
        MaterialApp(
          home: InvoiceDetailPage(
            invoice: abroad,
            accounting: ledger,
            tax: const _NoTax(),
            sales: sales,
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Post the sale to the ledger'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Post it'));
      await tester.pumpAndSettle();

      // the engine converts: the phone sends the dollars and the rate, never a converted figure of its own
      final sent = sales.posted.single;
      expect(sent['total'], '100000.00', reason: 'the customer\'s own figure');
      expect(sent['currency'], 'USD');
      expect(sent['rate'], '25.500000');
      expect(
        ledger.written,
        isEmpty,
        reason:
            'no line is built here, so a dollar sale can never land as Book money',
      );
      expect(find.textContaining('2550000.00'), findsWidgets);
    },
  );
}
