// Is the Book in order: what each check says, and what the page does when one cannot be made.
//
// The verdicts are pure functions over the doors' own answers, so they are tested with the figures those doors really send,
// and the page is tested with a source that answers or refuses on demand. No server, no emulator.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entries_list_page.dart';
import 'package:vimbai_mobile_client/features/close/data/close_report.dart';
import 'package:vimbai_mobile_client/features/close/data/close_source.dart';
import 'package:vimbai_mobile_client/features/close/presentation/books_in_order_page.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';

ExactAmount _money(String digits) => ExactAmount.fromJson(digits);

TrialBalance _balance({
  String debits = '1250.00',
  String credits = '1250.00',
  bool balanced = true,
}) => TrialBalance(
  reportDate: DateTime(2026, 11, 18),
  entries: const [],
  totalDebits: _money(debits),
  totalCredits: _money(credits),
  isBalanced: balanced,
);

JournalEntry _entry(String status) => JournalEntry(
  id: 'je-$status',
  entryDate: DateTime(2026, 11, 1),
  description: status,
  sourceModule: 'manual',
  status: status,
  lines: const [],
);

PayablesCheck _check({String control = '395.50', String suppliers = '395.50'}) {
  final a = _money(control);
  final b = _money(suppliers);
  final gap = a > b ? a - b : b - a;
  return PayablesCheck(
    controlAccount: a,
    supplierTotal: b,
    difference: gap,
    reconciled: gap.isZero,
  );
}

/// A Book that answers three readings, or refuses some, on demand.
class _Answers implements BookCloseSource {
  _Answers({this.balance, this.entries, this.check});

  final TrialBalance? balance;
  final List<JournalEntry>? entries;
  final PayablesCheck? check;

  @override
  Future<TrialBalance> trialBalance() async {
    if (balance == null) {
      throw Exception('the engine answered 503');
    }
    return balance!;
  }

  @override
  Future<List<JournalEntry>> journal() async {
    if (entries == null) {
      throw Exception('the engine answered 503');
    }
    return entries!;
  }

  @override
  Future<PayablesCheck> suppliersControl() async {
    if (check == null) {
      throw Exception('the engine answered 503');
    }
    return check!;
  }
}

void main() {
  group('the checks themselves', () {
    test('a balanced ledger reads as in order, with its own figures', () {
      final item = CloseItem.debitsAndCredits(_balance());
      expect(item.verdict, CloseVerdict.inOrder);
      expect(item.headline, contains('1250.00'));
    });

    test('a ledger that does not balance says the gap', () {
      final item = CloseItem.debitsAndCredits(
        _balance(debits: '1300.00', credits: '1250.00', balanced: false),
      );
      expect(item.verdict, CloseVerdict.needsYou);
      expect(item.headline, contains('50.00'));
    });

    test('drafts are counted, and one draft reads as one', () {
      final one = CloseItem.drafts([_entry('posted'), _entry('draft')]);
      expect(one.verdict, CloseVerdict.needsYou);
      expect(one.headline, contains('1 entry is'));

      final many = CloseItem.drafts([_entry('draft'), _entry('draft')]);
      expect(many.headline, contains('2 entries are'));
    });

    test('an empty journal is in order and says so plainly', () {
      final item = CloseItem.drafts(const []);
      expect(item.verdict, CloseVerdict.inOrder);
      expect(item.headline, contains('empty'));
    });

    test('suppliers agreeing with the control account is clean', () {
      expect(
        CloseItem.suppliersControl(_check()).verdict,
        CloseVerdict.inOrder,
      );
    });

    test('a control account that disagrees names both sides and the gap', () {
      final item = CloseItem.suppliersControl(
        _check(control: '400.00', suppliers: '395.50'),
      );
      expect(item.verdict, CloseVerdict.needsYou);
      expect(item.headline, contains('400.00'));
      expect(item.headline, contains('395.50'));
      expect(item.headline, contains('4.50'));
    });

    test('a check that could not be made is never counted as clean', () {
      final item = CloseItem.drafts(null, reason: 'the engine answered 503');
      expect(item.verdict, CloseVerdict.couldNotCheck);
      expect(item.detail, contains('503'));

      final report = CloseReport([item]);
      expect(report.isInOrder, isFalse);
    });

    test('drafts are the first thing worth fixing', () {
      final report = CloseReport([
        CloseItem.drafts([_entry('draft')]),
        CloseItem.suppliersControl(_check(control: '400.00', suppliers: '0')),
        CloseItem.debitsAndCredits(_balance(balanced: false)),
      ]);
      expect(report.firstNeedingYou?.subject, 'Entries still in draft');
      expect(report.needsYou, 3);
    });
  });

  group('the page', () {
    Widget page(BookCloseSource source) =>
        MaterialApp(home: BooksInOrderPage(source: source));

    testWidgets('a Book in order says so and shows three clean checks', (
      tester,
    ) async {
      await tester.pumpWidget(
        page(
          _Answers(
            balance: _balance(),
            entries: [_entry('posted')],
            check: _check(),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('This Book is in order'), findsOneWidget);
      expect(find.byIcon(Icons.check_circle_outline), findsNWidgets(3));
      expect(find.byIcon(Icons.error_outline), findsNothing);
    });

    testWidgets('the summary names the first thing to fix', (tester) async {
      await tester.pumpWidget(
        page(
          _Answers(
            balance: _balance(),
            entries: [_entry('draft')],
            check: _check(),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(
        find.textContaining('starting with entries still in draft'),
        findsOneWidget,
      );
      expect(find.byIcon(Icons.error_outline), findsOneWidget);
    });

    testWidgets('a check that failed is not shown as a clean tick', (
      tester,
    ) async {
      await tester.pumpWidget(
        page(
          _Answers(
            balance: _balance(),
            entries: [_entry('posted')],
            check: null, // the purchases ledger would not answer
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('could not be checked'), findsOneWidget);
      expect(find.textContaining('This Book is in order'), findsNothing);
      // the two that worked are still shown as clean
      expect(find.byIcon(Icons.check_circle_outline), findsNWidgets(2));
    });

    testWidgets('a check opens the screen that fixes it', (tester) async {
      await tester.pumpWidget(
        page(
          _Answers(
            balance: _balance(),
            entries: [_entry('draft')],
            check: _check(),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Entries still in draft'));
      // the journal screen starts its own read, which never answers here, so settle the route rather than the load
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));

      expect(find.byType(JournalEntriesListPage), findsOneWidget);
    });
  });
}
