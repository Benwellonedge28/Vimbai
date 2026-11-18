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
          home: InvoiceDetailPage(invoice: _invoice(), accounting: ledger),
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
          home: InvoiceDetailPage(invoice: _invoice(), accounting: ledger),
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
}
