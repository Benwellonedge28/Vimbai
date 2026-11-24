// Closing the year.
//
// What these pin: that the screen says what closing does before it is done, that it says the months have to be closed
// first, that it says plainly that nothing moves money, that a year already closed is refused before the engine is asked,
// that a year that does not read as a year is refused, that the engine's own refusal reaches the person, and that the
// engine's figures are repeated back rather than worked out here.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/year_end/data/year_end_posting.dart';
import 'package:vimbai_mobile_client/features/year_end/presentation/close_the_year_page.dart';

/// Stands in for the engine's year-end close: it remembers what it was asked to do.
class _FakePosting extends YearEndPostingService {
  _FakePosting({this.closed = const [], this.refusal = ''}) : super.fresh();

  List<ClosedYear> closed;

  /// When set, the next close is refused in these words, as the engine would.
  String refusal;

  final List<String> asked = [];

  @override
  Future<List<ClosedYear>> read({String year = ''}) async => closed;

  @override
  Future<ClosedYear> close({required String year, String notes = ''}) async {
    if (refusal.isNotEmpty) throw Exception(refusal);
    asked.add(year);
    return ClosedYear(
      year: year,
      entryDate: '${int.parse(year) + 1}-01-01',
      earned: '1000.000000',
      spent: '300.000000',
      profit: '700.000000',
      monthsReopenedSince: const [],
    );
  }
}

void main() {
  Future<void> open(
    WidgetTester tester,
    _FakePosting posting, {
    double height = 2400,
  }) async {
    tester.view.physicalSize = Size(1000, height);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      MaterialApp(home: CloseTheYearPage(posting: posting)),
    );
    await tester.pumpAndSettle();
  }

  testWidgets(
    'the screen says what closing does, that months come first, and that nothing moves',
    (tester) async {
      final posting = _FakePosting();
      await open(tester, posting);

      expect(
        find.textContaining('puts that figure in the owners\' equity'),
        findsOneWidget,
      );
      expect(
        find.textContaining('Close the year\'s months first'),
        findsOneWidget,
      );
      expect(find.textContaining('Nothing moves'), findsOneWidget);
      expect(posting.asked, isEmpty);
    },
  );

  testWidgets(
    'closing a year sends only the year, and repeats the engine\'s figures back',
    (tester) async {
      final posting = _FakePosting();
      await open(tester, posting);

      await tester.enterText(
        find.widgetWithText(TextField, 'Which year'),
        '2026',
      );
      await tester.tap(find.text('Close the year'));
      await tester.pumpAndSettle();

      // the phone sends the year and never a figure: the engine states what the year came to
      expect(posting.asked, ['2026']);
      expect(find.textContaining('2026: kept 700.000000'), findsOneWidget);
      expect(
        find.textContaining('earned 1000.000000, spent 300.000000'),
        findsOneWidget,
      );
      expect(find.textContaining('closing entry 2027-01-01'), findsOneWidget);
    },
  );

  testWidgets(
    'a year that does not read as a year is refused before it is sent',
    (tester) async {
      final posting = _FakePosting();
      await open(tester, posting);

      await tester.enterText(
        find.widgetWithText(TextField, 'Which year'),
        '26',
      );
      await tester.tap(find.text('Close the year'));
      await tester.pumpAndSettle();

      expect(find.textContaining('four digits'), findsOneWidget);
      expect(posting.asked, isEmpty);
    },
  );

  testWidgets('a year already closed is refused before the engine is asked', (
    tester,
  ) async {
    final posting = _FakePosting(
      closed: const [
        ClosedYear(
          year: '2026',
          entryDate: '2027-01-01',
          earned: '1000.000000',
          spent: '300.000000',
          profit: '700.000000',
          monthsReopenedSince: [],
        ),
      ],
    );
    await open(tester, posting);

    await tester.enterText(
      find.widgetWithText(TextField, 'Which year'),
      '2026',
    );
    await tester.tap(find.text('Close the year'));
    await tester.pumpAndSettle();

    expect(find.textContaining('has already been closed'), findsOneWidget);
    expect(posting.asked, isEmpty);
  });

  testWidgets(
    'the engine refusing a year whose months are open reaches the person in its own words',
    (tester) async {
      final posting = _FakePosting(
        refusal:
            '2026 is not ready to close: 2026-11, 2026-12 are still open. Close the months first.',
      );
      await open(tester, posting);

      await tester.enterText(
        find.widgetWithText(TextField, 'Which year'),
        '2026',
      );
      await tester.tap(find.text('Close the year'));
      await tester.pumpAndSettle();

      expect(find.textContaining('not ready to close'), findsOneWidget);
      expect(find.textContaining('2026-11'), findsOneWidget);
    },
  );

  testWidgets(
    'a Book that has closed no year says so, and a loss reads as a loss',
    (tester) async {
      await open(tester, _FakePosting());
      expect(find.textContaining('has not closed a year'), findsOneWidget);
    },
  );

  testWidgets(
    'a year that lost money is stated as a loss rather than a negative figure',
    (tester) async {
      final posting = _FakePosting(
        closed: const [
          ClosedYear(
            year: '2025',
            entryDate: '2026-01-01',
            earned: '200.000000',
            spent: '500.000000',
            profit: '-300.000000',
            monthsReopenedSince: [],
          ),
        ],
      );
      await open(tester, posting);

      expect(find.textContaining('2025: lost 300.000000'), findsOneWidget);
    },
  );

  testWidgets('a closed year whose month has since reopened says so', (
    tester,
  ) async {
    final posting = _FakePosting(
      closed: const [
        ClosedYear(
          year: '2025',
          entryDate: '2026-01-01',
          earned: '200.000000',
          spent: '500.000000',
          profit: '-300.000000',
          monthsReopenedSince: ['2025-07'],
        ),
      ],
    );
    await open(tester, posting);

    expect(find.textContaining('reopened since: 2025-07'), findsOneWidget);
  });
}
