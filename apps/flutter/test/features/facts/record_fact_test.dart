// Recording what happened, in the person's own words.
//
// What this pins is that the screen asks for a fact and not for two accounts: the words used are sent as they were typed,
// the rehearsal answers the posting without recording anything, and a refusal from the engine reaches the person in the
// engine's own words rather than as a status code.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/facts/data/fact_posting.dart';
import 'package:vimbai_mobile_client/features/facts/presentation/record_fact_page.dart';

/// Stands in for the engine: it remembers what it was asked, and answers with a posting.
class _FakeFacts extends FactPostingService {
  _FakeFacts({this.refusal, this.settles = false}) : super.fresh();

  /// When set, the engine refuses in these words instead of answering.
  final String? refusal;

  /// True when the engine read the money arriving as a payment against a debt rather than a new sale.
  final bool settles;

  final List<Map<String, Object?>> asked = [];

  Future<FactAnswer> _answer({
    required FactKind kind,
    required String entryDate,
    required ExactAmount amount,
    required String account,
    required String money,
    required String note,
    required bool rehearse,
  }) async {
    asked.add({
      'what': kind.wire,
      'date': entryDate,
      'amount': amount.toDecimalString(),
      'account': account,
      'money': money,
      'note': note,
      'rehearse': rehearse,
    });
    if (refusal != null) throw Exception(refusal);
    return FactAnswer(
      posted: !rehearse,
      kind: kind,
      entryDate: entryDate,
      debit: kind == FactKind.spent
          ? const FactSide(number: '6100', name: 'Rent')
          : const FactSide(number: '1010', name: 'Bank'),
      credit: kind == FactKind.spent
          ? const FactSide(number: '1010', name: 'Bank')
          : settles
          ? const FactSide(number: '1100', name: 'Accounts Receivable')
          : const FactSide(number: '4000', name: 'Sales'),
      amount: amount.toDecimalString(),
      description: 'Rent paid out of Bank',
      entryId: rehearse ? '' : 'entry-1',
      settledCustomer: settles ? 'acme' : '',
      settledDocument: settles ? 'INV-1' : '',
    );
  }

  @override
  Future<FactAnswer> rehearse({
    required FactKind kind,
    required String entryDate,
    required ExactAmount amount,
    required String account,
    String money = '',
    String note = '',
  }) => _answer(
    kind: kind,
    entryDate: entryDate,
    amount: amount,
    account: account,
    money: money,
    note: note,
    rehearse: true,
  );

  @override
  Future<FactAnswer> post({
    required FactKind kind,
    required String entryDate,
    required ExactAmount amount,
    required String account,
    String money = '',
    String note = '',
  }) => _answer(
    kind: kind,
    entryDate: entryDate,
    amount: amount,
    account: account,
    money: money,
    note: note,
    rehearse: false,
  );
}

Future<void> _fill(WidgetTester tester, String label, String text) async {
  await tester.enterText(find.widgetWithText(TextField, label), text);
  await tester.pumpAndSettle();
}

Future<void> _press(WidgetTester tester, String label) async {
  await tester.tap(find.text(label).last);
  await tester.pumpAndSettle();
}

Future<_FakeFacts> _screen(
  WidgetTester tester, {
  String? refusal,
  bool settles = false,
}) async {
  // a tall surface so the answer and the complaint are built rather than waiting below the fold
  tester.view.physicalSize = const Size(1000, 2200);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
  final facts = _FakeFacts(refusal: refusal, settles: settles);
  await tester.pumpWidget(MaterialApp(home: RecordFactPage(posting: facts)));
  await tester.pumpAndSettle();
  return facts;
}

void main() {
  testWidgets('money going out is asked for as a fact, not as two accounts', (
    tester,
  ) async {
    final facts = await _screen(tester);
    await _fill(tester, 'How much', '40.00');
    await _fill(tester, 'What it was spent on', 'rent');
    await _press(tester, 'Record it');

    expect(facts.asked, hasLength(1));
    expect(facts.asked.single['what'], FactKind.spent.wire);
    expect(facts.asked.single['account'], 'rent');
    expect(facts.asked.single['amount'], '40.00');
    expect(facts.asked.single['rehearse'], isFalse);
    expect(find.text('Recorded'), findsOneWidget);
    expect(find.text('The books hold it now.'), findsOneWidget);
  });

  testWidgets('money coming in is the other fact, and says so', (tester) async {
    final facts = await _screen(tester);
    await tester.tap(find.text('Money came in'));
    await tester.pumpAndSettle();
    await _fill(tester, 'How much', '500');
    await _fill(tester, 'Who paid, or what it came in for', 'sales');
    await _press(tester, 'Record it');

    expect(facts.asked.single['what'], FactKind.received.wire);
    expect(find.text('Sales (4000)'), findsOneWidget);
  });

  testWidgets('money from a customer who owes says which debt it cleared', (
    tester,
  ) async {
    final facts = await _screen(tester, settles: true);
    await tester.tap(find.text('Money came in'));
    await tester.pumpAndSettle();
    await _fill(tester, 'How much', '400');
    await _fill(tester, 'Who paid, or what it came in for', 'acme');
    await _press(tester, 'Record it');

    // the same field takes a customer, and the answer names the debt rather than pretending a new sale was earned
    expect(facts.asked.single['account'], 'acme');
    expect(find.text('Clears'), findsOneWidget);
    expect(find.text("acme's INV-1"), findsOneWidget);
    expect(find.text('Accounts Receivable (1100)'), findsOneWidget);
  });

  testWidgets('showing the entry records nothing and says so', (tester) async {
    final facts = await _screen(tester);
    await _fill(tester, 'How much', '40.00');
    await _fill(tester, 'What it was spent on', 'rent');
    await _press(tester, 'Show me the entry');

    expect(facts.asked.single['rehearse'], isTrue);
    expect(find.text('This is what would be posted'), findsOneWidget);
    expect(find.text('Nothing has been recorded yet.'), findsOneWidget);
    expect(find.text('Rent (6100)'), findsOneWidget);
    expect(find.text('Bank (1010)'), findsOneWidget);
    expect(find.text('Recorded'), findsNothing);
  });

  testWidgets('the engine\'s refusal reaches the person in its own words', (
    tester,
  ) async {
    await _screen(
      tester,
      refusal:
          'This Book has no cost called "aeroplane". It has 6100 Rent, 6300 Office Expenses.',
    );
    await _fill(tester, 'How much', '40.00');
    await _fill(tester, 'What it was spent on', 'aeroplane');
    await _press(tester, 'Record it');

    expect(find.textContaining('no cost called "aeroplane"'), findsOneWidget);
    expect(find.text('Recorded'), findsNothing);
  });

  testWidgets('an amount of nothing is refused before the engine is asked', (
    tester,
  ) async {
    final facts = await _screen(tester);
    await _fill(tester, 'What it was spent on', 'rent');
    await _press(tester, 'Record it');

    expect(facts.asked, isEmpty);
    expect(find.text('Say how much, like 40.50.'), findsOneWidget);
  });

  testWidgets('a fact that does not say what it was for asks for it', (
    tester,
  ) async {
    final facts = await _screen(tester);
    await _fill(tester, 'How much', '40.00');
    await _press(tester, 'Record it');

    expect(facts.asked, isEmpty);
    expect(find.textContaining('Say What it was spent on'), findsOneWidget);
  });

  testWidgets('the account the money moved through is sent when named', (
    tester,
  ) async {
    final facts = await _screen(tester);
    await _fill(tester, 'How much', '40.00');
    await _fill(tester, 'What it was spent on', 'rent');
    await _fill(tester, 'Which account the money moved through', 'till');
    await _press(tester, 'Record it');

    expect(facts.asked.single['money'], 'till');
  });
}
