// Find one thing in this Book.
//
// What these pin: that nothing is searched until there are two letters, that the engine's answer is drawn as it came and
// grouped into entries and accounts, that a hit says which part of it matched and a draft says it is not in the statements,
// that a search which found nothing says so rather than looking broken, and that a failed read is a failure rather than
// "nothing found".
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/find/data/find_service.dart';
import 'package:vimbai_mobile_client/features/find/presentation/find_page.dart';

/// Stands in for the engine's search: it remembers what it was asked.
class _FakeFind extends FindService {
  _FakeFind({this.answer, this.refusal = ''}) : super.fresh();

  FindAnswer? answer;

  /// When set, the next search is refused in these words, as the engine would.
  String refusal;

  /// How long a search for a given query takes, so a test can make one answer arrive after another.
  Map<String, Duration> delays = const {};

  final List<String> asked = [];

  @override
  Future<FindAnswer> find(String words) async {
    asked.add(words);
    final pause = delays[words];
    if (pause != null) await Future<void>.delayed(pause);
    if (refusal.isNotEmpty) throw Exception(refusal);
    // an answer belongs to the words it was asked for, as the engine's does
    if (words == 'rent') return _anAnswer(words);
    return answer ??
        FindAnswer(query: words, entries: const [], accounts: const []);
  }
}

FindAnswer _anAnswer(String query) => FindAnswer(
  query: query,
  entries: [
    FoundThing(
      kind: 'entry',
      id: 'entry-1',
      title: 'Sold a car to Tendai',
      subtitle: 'INV-2 · 2026-11-02 · posted',
      matchedOn: 'reference',
      status: 'posted',
    ),
    FoundThing(
      kind: 'entry',
      id: 'entry-2',
      title: 'Owed for the yard rent',
      subtitle: 'DR-1 · 2026-11-05 · draft',
      matchedOn: 'description',
      status: 'draft',
    ),
  ],
  accounts: const [
    FoundThing(
      kind: 'account',
      id: '1010',
      title: 'Bank',
      subtitle: '1010 · asset',
      matchedOn: 'name',
      status: '',
    ),
  ],
);

void main() {
  Future<void> open(
    WidgetTester tester,
    _FakeFind service, {
    double height = 2000,
  }) async {
    tester.view.physicalSize = Size(1000, height);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(MaterialApp(home: FindPage(service: service)));
    await tester.pumpAndSettle();
  }

  testWidgets('nothing is searched until there are two letters', (
    tester,
  ) async {
    final service = _FakeFind();
    await open(tester, service);

    await tester.enterText(find.byType(TextField), 'i');
    await tester.pump(const Duration(milliseconds: 500));
    expect(service.asked, isEmpty, reason: 'one letter is not a search');

    await tester.enterText(find.byType(TextField), 'IN');
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpAndSettle();
    expect(service.asked, ['IN']);
  });

  testWidgets(
    'the engine answer is drawn grouped, and a draft says it is not in the statements',
    (tester) async {
      final service = _FakeFind(answer: _anAnswer('IN'));
      await open(tester, service);

      await tester.enterText(find.byType(TextField), 'IN');
      await tester.pump(const Duration(milliseconds: 500));
      await tester.pumpAndSettle();

      expect(find.text('Entries'), findsOneWidget);
      expect(find.text('Accounts'), findsOneWidget);
      expect(find.text('Sold a car to Tendai'), findsOneWidget);
      expect(find.textContaining('INV-2'), findsOneWidget);
      // a draft is found and says plainly that the statements do not hold it yet
      expect(find.textContaining('not in the statements yet'), findsOneWidget);
      expect(find.text('Bank'), findsOneWidget);
    },
  );

  testWidgets(
    'a search that found nothing says so rather than looking broken',
    (tester) async {
      final service = _FakeFind();
      await open(tester, service);

      await tester.enterText(find.byType(TextField), 'nothing at all');
      await tester.pump(const Duration(milliseconds: 500));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('Nothing in this Book holds "nothing at all"'),
        findsOneWidget,
      );
    },
  );

  testWidgets('the engine refusing a query reaches the person in its own words', (
    tester,
  ) async {
    final service = _FakeFind(
      refusal:
          'Say at least 2 letters to search for: one letter matches nearly everything in the Book.',
    );
    await open(tester, service);

    await tester.enterText(find.byType(TextField), 'ab');
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpAndSettle();

    expect(find.textContaining('at least 2 letters'), findsOneWidget);
  });

  testWidgets('a slower answer to older words is dropped rather than drawn', (
    tester,
  ) async {
    final service = _FakeFind()
      ..delays = {'rent': const Duration(milliseconds: 400)};
    await open(tester, service);

    // the search for "rent" is answered slowly, and the person keeps typing while it is in flight
    await tester.enterText(find.byType(TextField), 'rent');
    await tester.pump(const Duration(milliseconds: 400));
    await tester.enterText(find.byType(TextField), 'rent co');
    await tester.pump(const Duration(milliseconds: 400));
    // the older answer lands here, and must not be drawn: it answers words that are no longer in the box
    await tester.pump(const Duration(milliseconds: 600));
    await tester.pumpAndSettle();

    expect(service.asked, ['rent', 'rent co']);
    expect(find.textContaining('Sold a car to Tendai'), findsNothing);
    expect(
      find.textContaining('Nothing in this Book holds "rent co"'),
      findsOneWidget,
    );
  });
}
