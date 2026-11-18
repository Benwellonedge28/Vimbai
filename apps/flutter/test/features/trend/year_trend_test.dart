// How the year has gone: what each month says, and what the page refuses to claim when a month did not answer.
//
// The trend itself is pure arithmetic over the engine's monthly income statements, so it is tested with the figures those
// statements carry. The page is tested with a source that answers or refuses per month, because the interesting behaviour is
// that one unread month withholds the year total instead of quietly becoming a month of zero.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/income_statement_page.dart';
import 'package:vimbai_mobile_client/features/trend/data/month_trend.dart';
import 'package:vimbai_mobile_client/features/trend/presentation/year_trend_page.dart';

ExactAmount _money(String digits) => ExactAmount.fromJson(digits);

IncomeStatement _statement({
  String revenue = '1000.00',
  String expense = '600.00',
  String net = '400.00',
}) => IncomeStatement(
  startDate: DateTime(2026, 1, 1),
  endDate: DateTime(2026, 1, 31),
  revenues: [IncomeStatementItem(category: 'Sales', amount: _money(revenue))],
  expenses: [IncomeStatementItem(category: 'Rent', amount: _money(expense))],
  netIncome: _money(net),
);

MonthFigure _month(int month, String income, String expense, String profit) =>
    MonthFigure(
      month: DateTime(2026, month),
      income: _money(income),
      expenses: _money(expense),
      profit: _money(profit),
    );

/// A Book whose months answer, or refuse, by month number.
class _Months implements YearTrendSource {
  _Months(this.answers, {this.refuse = const <int>{}});

  final Map<int, IncomeStatement> answers;
  final Set<int> refuse;

  @override
  Future<IncomeStatement> month(DateTime start, DateTime end) async {
    if (refuse.contains(start.month) || !answers.containsKey(start.month)) {
      throw Exception(
        'the engine answered 500 for ${monthNames[start.month - 1]}',
      );
    }
    return answers[start.month]!;
  }
}

void main() {
  group('the months asked for', () {
    test('January to the month that holds today, and no part month', () {
      final months = monthsSoFar(DateTime(2026, 11, 18));
      expect(months.length, 11);
      expect(months.first, DateTime(2026, 1));
      expect(months.last, DateTime(2026, 11));
    });

    test('a month ends on its own last day', () {
      expect(endOfMonth(DateTime(2026, 2)), DateTime(2026, 2, 28));
      expect(endOfMonth(DateTime(2024, 2)), DateTime(2024, 2, 29));
      expect(endOfMonth(DateTime(2026, 12)), DateTime(2026, 12, 31));
    });
  });

  group('the year', () {
    test(
      'a month is its statement: the lines are in, and net income is what stayed',
      () {
        final month = MonthFigure.of(DateTime(2026, 3), _statement());
        expect(month.income, _money('1000.00'));
        expect(month.expenses, _money('600.00'));
        expect(month.profit, _money('400.00'));
        expect(month.wasRead, isTrue);
        expect(month.name, 'March');
      },
    );

    test('the year adds the months that answered', () {
      final trend = YearTrend([
        _month(1, '1000.00', '600.00', '400.00'),
        _month(2, '500.00', '900.00', '-400.00'),
      ]);
      expect(trend.isWhole, isTrue);
      expect(trend.income, _money('1500.00'));
      expect(trend.expenses, _money('1500.00'));
      expect(trend.profit, _money('0.00'));
      expect(trend.best?.month.month, 1);
      expect(trend.worst?.month.month, 2);
      expect(trend.losing.length, 1);
    });

    test('one unread month is not a month of zero', () {
      final trend = YearTrend([
        _month(1, '1000.00', '600.00', '400.00'),
        MonthFigure.unread(DateTime(2026, 2)),
      ]);
      expect(trend.isWhole, isFalse);
      expect(trend.unread, 1);
      // the read month is still counted, but the page is told the year is not whole
      expect(trend.profit, _money('400.00'));
      expect(trend.best?.name, 'January');
    });

    test('a quiet month is a real answer, not a missing one', () {
      final quiet = _month(2, '0.00', '0.00', '0.00');
      expect(quiet.wasRead, isTrue);
      expect(quiet.wasQuiet, isTrue);

      final unread = MonthFigure.unread(DateTime(2026, 2));
      expect(unread.wasQuiet, isFalse);
      expect(unread.wasRead, isFalse);
    });

    test(
      'the bars are drawn to the largest figure of the year, losses included',
      () {
        final trend = YearTrend([
          _month(1, '100.00', '50.00', '50.00'),
          _month(2, '80.00', '900.00', '-820.00'),
        ]);
        expect(trend.largest, _money('900.00'));
      },
    );
  });

  group('the page', () {
    Widget page(YearTrendSource source) => MaterialApp(
      home: YearTrendPage(source: source, today: DateTime(2026, 3, 15)),
    );

    testWidgets('the year reads month by month with a total', (tester) async {
      await tester.pumpWidget(
        page(
          _Months({
            1: _statement(revenue: '1000.00', expense: '600.00', net: '400.00'),
            2: _statement(revenue: '500.00', expense: '900.00', net: '-400.00'),
            3: _statement(revenue: '700.00', expense: '200.00', net: '500.00'),
          }),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('The year so far'), findsOneWidget);
      expect(find.textContaining('2200.00 in, 1700.00 out'), findsOneWidget);
      expect(find.textContaining('a profit of 500.00'), findsOneWidget);
      expect(find.text('January'), findsOneWidget);
      expect(find.text('March'), findsOneWidget);
      expect(find.textContaining('March was the strongest'), findsOneWidget);
    });

    testWidgets('one month that failed withholds the year total', (
      tester,
    ) async {
      await tester.pumpWidget(
        page(
          _Months(
            {
              1: _statement(
                revenue: '1000.00',
                expense: '600.00',
                net: '400.00',
              ),
              3: _statement(
                revenue: '700.00',
                expense: '200.00',
                net: '500.00',
              ),
            },
            refuse: {2},
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('could not be read'), findsWidgets);
      expect(find.textContaining('2 of 3 months read'), findsOneWidget);
      expect(find.textContaining('The year so far'), findsNothing);
    });

    testWidgets('tapping a month opens the statement behind it', (
      tester,
    ) async {
      await tester.pumpWidget(
        page(_Months({1: _statement(), 2: _statement(), 3: _statement()})),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('January'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));

      expect(find.byType(IncomeStatementPage), findsOneWidget);
    });
  });
}
