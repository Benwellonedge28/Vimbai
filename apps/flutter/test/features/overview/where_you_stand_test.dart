// Where you stand: the four figures, and what the page does when one of them is missing.
//
// The page is tested without a server by handing it a source that answers, so what is being checked is the page's own
// behaviour - the figures it prints, that a failed read is shown in place rather than blanking the other three, and that each
// card leads to the screen that owns the figure.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/cash_book_page.dart';
import 'package:vimbai_mobile_client/features/overview/data/stand_source.dart';
import 'package:vimbai_mobile_client/features/overview/presentation/where_you_stand_page.dart';

/// A Book that answers four figures, or refuses some of them, on demand.
class _Ask implements WhereYouStandSource {
  _Ask({this.refuse = const <String>{}});

  /// Money held, before the test that asks twice changes it.
  final String held = '1000.00';

  /// The other three, fixed: this page only prints them.
  final String owed = '250.00';
  final String owing = '400.00';
  final String profit = '900.00';

  /// The figures this Book will not answer, by name.
  final Set<String> refuse;

  ExactAmount _orRefuse(String figure, String digits) {
    if (refuse.contains(figure)) {
      throw Exception('the engine answered 503');
    }
    return ExactAmount.fromJson(digits);
  }

  @override
  Future<ExactAmount> moneyHeld() async => _orRefuse('held', held);

  @override
  Future<ExactAmount> owedToYou() async => _orRefuse('owed', owed);

  @override
  Future<ExactAmount> youOwe() async => _orRefuse('owing', owing);

  @override
  Future<ExactAmount> profitThisYear() async => _orRefuse('profit', profit);
}

Widget _page(WhereYouStandSource source) =>
    MaterialApp(home: WhereYouStandPage(source: source, year: 2026));

void main() {
  testWidgets('the four figures are printed as the doors answered them', (
    tester,
  ) async {
    await tester.pumpWidget(_page(_Ask()));
    await tester.pumpAndSettle();

    expect(find.text('1000.00'), findsOneWidget); // money held
    expect(find.text('250.00'), findsOneWidget); // owed to you
    expect(find.text('400.00'), findsOneWidget); // you owe
    expect(find.text('900.00'), findsOneWidget); // profit
  });

  testWidgets('the opening sentence counts what can be counted today', (
    tester,
  ) async {
    await tester.pumpWidget(_page(_Ask()));
    await tester.pumpAndSettle();

    // 1000 held + 250 to come in - 400 to go out
    expect(
      find.textContaining('Counted together that is 850.00'),
      findsOneWidget,
    );
  });

  testWidgets('a figure that could not be read is named, and the rest stand', (
    tester,
  ) async {
    await tester.pumpWidget(_page(_Ask(refuse: {'owing'})));
    await tester.pumpAndSettle();

    expect(
      find.textContaining('The purchases ledger could not be read just now.'),
      findsOneWidget,
    );
    // the reason is passed on in the engine's own words, without the exception scaffold
    expect(find.textContaining('the engine answered 503'), findsOneWidget);
    // and the other three are still there, which is the whole point of a card failing instead of the page
    expect(find.text('1000.00'), findsOneWidget);
    expect(find.text('250.00'), findsOneWidget);
    expect(find.text('900.00'), findsOneWidget);
  });

  testWidgets('a page with a missing figure does not claim a total', (
    tester,
  ) async {
    await tester.pumpWidget(_page(_Ask(refuse: {'owed'})));
    await tester.pumpAndSettle();

    expect(find.textContaining('Counted together'), findsNothing);
  });

  testWidgets('a card opens the screen that owns its figure', (tester) async {
    await tester.pumpWidget(_page(_Ask()));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Money held'));
    await tester.pumpAndSettle();

    expect(find.byType(CashBookPage), findsOneWidget);
  });

  testWidgets('asking again reads the four figures again', (tester) async {
    final changing = _Changes();
    await tester.pumpWidget(_page(changing));
    await tester.pumpAndSettle();
    expect(find.text('1000.00'), findsOneWidget);

    // the money moved between the two reads, and asking again is what brings it in
    await tester.tap(find.byTooltip('Ask again'));
    await tester.pumpAndSettle();
    expect(find.text('1200.00'), findsOneWidget);
    expect(find.text('1000.00'), findsNothing);
  });
}

/// A Book whose money changes after the first answer, so a second read can be told from the first.
class _Changes extends _Ask {
  int _asked = 0;

  @override
  Future<ExactAmount> moneyHeld() async {
    _asked += 1;
    return ExactAmount.fromJson(_asked == 1 ? '1000.00' : '1200.00');
  }
}
