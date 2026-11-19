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
import 'package:vimbai_mobile_client/features/invoicing/presentation/invoice_detail_page.dart';
import 'package:vimbai_mobile_client/features/tax/data/sale_tax.dart';
import 'package:vimbai_mobile_client/features/tax/data/tax_table.dart';

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
  }) async => throw UnimplementedError('nothing was chosen');
}

/// A Book that charges VAT at 15.5%, answering with the engine's own figures for a 100,000.00 sale.
class _VatBook implements SaleTaxReads {
  _VatBook(this.gross);

  final ExactAmount gross;
  final List<String> asked = [];

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
  }) async {
    asked.addAll(taxes);
    final total = ExactAmount.parse(gross);
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
}
