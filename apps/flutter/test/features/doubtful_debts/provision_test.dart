// What the business expects not to be paid.
//
// What these pin: that the screen says what raising and lowering do before anything is posted, that it says no customer's
// debt leaves the books and that nothing moves money, that it sends the figure the person states rather than working out a
// percentage, that a figure already standing is refused before the engine is asked, that the engine's own refusal reaches
// the person, and that what stands today leads the ladder of estimates.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/doubtful_debts/data/doubtful_debts_posting.dart';
import 'package:vimbai_mobile_client/features/doubtful_debts/presentation/provision_page.dart';

/// Stands in for the engine's doubtful debts journal: it remembers what it was asked to do.
class _FakePosting extends DoubtfulDebtsPostingService {
  _FakePosting({
    this.standing = '',
    this.adjustments = const [],
    this.refusal = '',
  }) : super.fresh();

  String standing;
  List<ProvisionAdjustment> adjustments;

  /// When set, the next write is refused in these words, as the engine would.
  String refusal;

  final List<Map<String, Object?>> posted = [];

  @override
  Future<ProvisionLadder> read({String period = ''}) async =>
      ProvisionLadder(standing: standing, adjustments: adjustments);

  @override
  Future<ProvisionAdjustment> provide({
    required String entryDate,
    required String provision,
    String reason = '',
    String notes = '',
  }) async {
    if (refusal.isNotEmpty) throw Exception(refusal);
    posted.add({
      'entry_date': entryDate,
      'provision': provision,
      'reason': reason,
    });
    return ProvisionAdjustment(
      entryDate: entryDate,
      provision: provision,
      previous: standing.isEmpty ? '0.000000' : standing,
      movement: provision,
      direction: 'raised',
      reason: reason,
    );
  }
}

void main() {
  Future<void> open(
    WidgetTester tester,
    _FakePosting posting, {
    double height = 2800,
  }) async {
    tester.view.physicalSize = Size(1000, height);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(MaterialApp(home: ProvisionPage(posting: posting)));
    await tester.pumpAndSettle();
  }

  Future<void> state(WidgetTester tester, String figure) async {
    await tester.enterText(
      find.widgetWithText(TextField, 'What the provision should stand at'),
      figure,
    );
    await tester.pumpAndSettle();
  }

  testWidgets(
    'the screen says what raising and lowering do, and that nothing moves',
    (tester) async {
      final posting = _FakePosting();
      await open(tester, posting);

      expect(
        find.textContaining(
          'charges the month and grows what stands against receivables',
        ),
        findsOneWidget,
      );
      expect(
        find.textContaining('takes the earlier charge back'),
        findsOneWidget,
      );
      expect(
        find.textContaining('No customer\'s debt leaves the books'),
        findsOneWidget,
      );
      expect(find.textContaining('nothing moves money'), findsOneWidget);
      expect(posting.posted, isEmpty);
    },
  );

  testWidgets(
    'the estimate the person states is what is sent, and the screen repeats the engine',
    (tester) async {
      final posting = _FakePosting();
      await open(tester, posting);

      await tester.enterText(
        find.widgetWithText(TextField, 'The day it was decided'),
        '2026-11-20',
      );
      await state(tester, '450.00');
      await tester.enterText(
        find.widgetWithText(TextField, 'Why'),
        'Two customers have gone quiet',
      );
      await tester.tap(find.text('Record the estimate'));
      await tester.pumpAndSettle();

      // the phone sends the figure the person decided on: it never works out a percentage of what is owed
      expect(posting.posted.single, {
        'entry_date': '2026-11-20',
        'provision': '450.00',
        'reason': 'Two customers have gone quiet',
      });
      expect(find.textContaining('raised by 450.00 to 450.00'), findsOneWidget);
    },
  );

  testWidgets(
    'a figure already standing is refused before the engine is asked',
    (tester) async {
      final posting = _FakePosting(standing: '450.000000');
      await open(tester, posting);

      await state(tester, '450.00');
      await tester.tap(find.text('Record the estimate'));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('already stands at 450.000000'),
        findsOneWidget,
      );
      expect(posting.posted, isEmpty);
    },
  );

  testWidgets(
    'a missing figure, or a figure of nothing, is refused in fixing words',
    (tester) async {
      final posting = _FakePosting();
      await open(tester, posting);

      await tester.tap(find.text('Record the estimate'));
      await tester.pumpAndSettle();
      expect(
        find.textContaining('Say what the provision should stand at'),
        findsOneWidget,
      );

      await state(tester, '0');
      await tester.tap(find.text('Record the estimate'));
      await tester.pumpAndSettle();
      expect(find.textContaining('more than nothing'), findsOneWidget);
      expect(posting.posted, isEmpty);
    },
  );

  testWidgets(
    'the engine refusing a second estimate on the same day reaches the person',
    (tester) async {
      final posting = _FakePosting(
        refusal:
            'Journal Entry with reference PROV-2026-11-20 already exists for source DoubtfulDebts',
      );
      await open(tester, posting);

      await state(tester, '450.00');
      await tester.tap(find.text('Record the estimate'));
      await tester.pumpAndSettle();

      expect(find.textContaining('already exists'), findsOneWidget);
    },
  );

  testWidgets('a Book that has provided for nothing says so', (tester) async {
    await open(tester, _FakePosting());
    expect(
      find.textContaining('has not provided for anything'),
      findsOneWidget,
    );
    expect(
      find.textContaining('Nothing stands against receivables yet'),
      findsOneWidget,
    );
  });

  testWidgets(
    'what stands today leads, and the ladder of estimates is read back',
    (tester) async {
      final posting = _FakePosting(
        standing: '700.000000',
        adjustments: const [
          ProvisionAdjustment(
            entryDate: '2026-12-18',
            provision: '700.000000',
            previous: '450.000000',
            movement: '250.000000',
            direction: 'raised',
            reason: 'A big one is in dispute',
          ),
          ProvisionAdjustment(
            entryDate: '2026-11-20',
            provision: '450.000000',
            previous: '0.000000',
            movement: '450.000000',
            direction: 'raised',
            reason: '',
          ),
        ],
      );
      await open(tester, posting, height: 4200);

      expect(find.textContaining('stands at 700.000000 today'), findsOneWidget);
      expect(
        find.textContaining('raised by 250.000000 to 700.000000'),
        findsOneWidget,
      );
      expect(find.textContaining('A big one is in dispute'), findsOneWidget);
    },
  );
}
