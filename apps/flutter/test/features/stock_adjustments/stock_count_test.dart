// What a count found the stock to be.
//
// What these pin: that the screen says what each way does to the books before it is recorded, that it states the
// difference in money rather than working a quantity out, that a shortage and a surplus are told apart in what is sent,
// that a missing item or a figure of nothing is refused in fixing words before the engine is asked, that the engine's own
// refusal reaches the person, and that the screen says plainly that nothing moves money.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/stock_adjustments/data/stock_adjustments_posting.dart';
import 'package:vimbai_mobile_client/features/stock_adjustments/presentation/stock_count_page.dart';

/// Stands in for the engine's stock adjustments journal: it remembers what it was asked to do.
class _FakePosting extends StockAdjustmentsPostingService {
  _FakePosting({this.found = const [], this.refusal = ''}) : super.fresh();

  List<StockAdjustment> found;

  /// When set, the next write is refused in these words, as the engine would.
  String refusal;

  final List<Map<String, Object?>> posted = [];

  @override
  Future<List<StockAdjustment>> read({
    String item = '',
    String period = '',
  }) async => found;

  @override
  Future<StockAdjustment> post({
    required String entryDate,
    required String item,
    required String direction,
    required String amount,
    String reason = '',
    String notes = '',
  }) async {
    if (refusal.isNotEmpty) throw Exception(refusal);
    posted.add({
      'entry_date': entryDate,
      'item': item,
      'direction': direction,
      'amount': amount,
      'reason': reason,
    });
    return StockAdjustment(
      entryDate: entryDate,
      item: item,
      direction: direction,
      amount: amount,
      reason: reason,
    );
  }
}

void main() {
  /// The screen as a person sees it, on a surface tall enough that the whole form is built.
  Future<void> open(
    WidgetTester tester,
    _FakePosting posting, {
    double height = 2800,
  }) async {
    tester.view.physicalSize = Size(1000, height);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      MaterialApp(home: StockCountPage(posting: posting)),
    );
    await tester.pumpAndSettle();
  }

  Future<void> state(
    WidgetTester tester, {
    required String item,
    required String figure,
  }) async {
    await tester.enterText(
      find.widgetWithText(TextField, 'What was counted'),
      item,
    );
    await tester.enterText(
      find.widgetWithText(TextField, 'What the difference comes to'),
      figure,
    );
    await tester.pumpAndSettle();
  }

  testWidgets(
    'the screen says what a shortage does to the books and that nothing moves',
    (tester) async {
      final posting = _FakePosting();
      await open(tester, posting);

      expect(
        find.textContaining('the month carries the cost of it'),
        findsOneWidget,
      );
      expect(
        find.textContaining('No supplier owes anything, and nothing was sold'),
        findsOneWidget,
      );
      expect(
        find.textContaining(
          'counting stock never pays anybody or collects anything',
        ),
        findsOneWidget,
      );
      expect(find.text('Record what the count found'), findsOneWidget);
      expect(posting.posted, isEmpty);
    },
  );

  testWidgets(
    'a shortage is sent as down, with the item, the day and the money the count states',
    (tester) async {
      final posting = _FakePosting();
      await open(tester, posting);

      await tester.enterText(
        find.widgetWithText(TextField, 'The day it was counted'),
        '2026-11-20',
      );
      await state(tester, item: 'Cement 50kg', figure: '180.00');
      await tester.enterText(
        find.widgetWithText(TextField, 'Why'),
        'Water damage',
      );
      await tester.tap(find.text('Record what the count found'));
      await tester.pumpAndSettle();

      expect(posting.posted, hasLength(1));
      // the phone states the figure the count gave, and never works one out from a quantity and a price
      expect(posting.posted.single, {
        'entry_date': '2026-11-20',
        'item': 'Cement 50kg',
        'direction': 'down',
        'amount': '180.00',
        'reason': 'Water damage',
      });
      // and the screen reads the count back
      expect(find.text('Cement 50kg'), findsOneWidget);
      expect(find.textContaining('less than the books'), findsOneWidget);
    },
  );

  testWidgets(
    'stock found is sent as up, and the screen says what that does before it is recorded',
    (tester) async {
      final posting = _FakePosting();
      await open(tester, posting);

      await tester.tap(find.text('More than the books'));
      await tester.pumpAndSettle();
      expect(
        find.textContaining('the cost carried earlier comes back with it'),
        findsOneWidget,
      );

      await state(tester, item: 'Cement 50kg', figure: '60.00');
      await tester.tap(find.text('Record what the count found'));
      await tester.pumpAndSettle();

      expect(posting.posted.single['direction'], 'up');
      expect(find.textContaining('more than the books'), findsOneWidget);
    },
  );

  testWidgets(
    'a count with no item, or a difference of nothing, is refused before it is sent',
    (tester) async {
      final posting = _FakePosting();
      await open(tester, posting);

      await tester.enterText(
        find.widgetWithText(TextField, 'What the difference comes to'),
        '180.00',
      );
      await tester.tap(find.text('Record what the count found'));
      await tester.pumpAndSettle();
      expect(find.textContaining('Say what was counted'), findsOneWidget);
      expect(posting.posted, isEmpty);

      await tester.enterText(
        find.widgetWithText(TextField, 'What was counted'),
        'Sugar',
      );
      await tester.enterText(
        find.widgetWithText(TextField, 'What the difference comes to'),
        '0',
      );
      await tester.tap(find.text('Record what the count found'));
      await tester.pumpAndSettle();
      expect(find.textContaining('more than nothing'), findsOneWidget);
      expect(posting.posted, isEmpty);
    },
  );

  testWidgets(
    'the engine refusing a count already recorded reaches the person in its own words',
    (tester) async {
      final posting = _FakePosting(
        refusal:
            'Journal Entry with reference STK-2026-11-20-Cement 50kg already exists for source StockAdjustments',
      );
      await open(tester, posting);

      await state(tester, item: 'Cement 50kg', figure: '180.00');
      await tester.tap(find.text('Record what the count found'));
      await tester.pumpAndSettle();

      expect(find.textContaining('already exists'), findsOneWidget);
    },
  );

  testWidgets('a Book that has recorded no count says so', (tester) async {
    await open(tester, _FakePosting());
    expect(find.textContaining('has not recorded a count'), findsOneWidget);
  });

  testWidgets(
    'what counting has already found is read back with the way it went',
    (tester) async {
      final posting = _FakePosting(
        found: const [
          StockAdjustment(
            entryDate: '2026-11-20',
            item: 'Cement 50kg',
            direction: 'down',
            amount: '180.000000',
            reason: 'Water damage',
          ),
        ],
      );
      // a taller surface, so the list of what counting has found is built as well as the form
      await open(tester, posting, height: 4200);

      expect(find.text('Cement 50kg'), findsOneWidget);
      expect(find.textContaining('Water damage'), findsOneWidget);
      expect(find.textContaining('less than the books'), findsOneWidget);
    },
  );
}
