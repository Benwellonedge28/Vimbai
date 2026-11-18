// How the year has gone: one month at a time, from the engine's own income statements.
//
// A year-to-date profit is a single number that hides the shape of the year: two good months and ten quiet ones reads the
// same as twelve steady ones. This asks the engine for each month separately and puts them side by side, so a person can see
// whether the business is steady, growing, or carried by one bumper month.
//
// A month that could not be read is not a month of zero. It is held as unknown, and while any month is unknown the year total
// is withheld rather than printed from the months that did answer - the same rule the overview page follows.
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';

const List<String> monthNames = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
];

/// One month of the year, or the fact that it could not be read.
class MonthFigure {
  const MonthFigure({
    required this.month,
    required this.income,
    required this.expenses,
    required this.profit,
  });

  /// The first day of the month, so the month is named without a second field to keep in step.
  final DateTime month;

  final ExactAmount? income;
  final ExactAmount? expenses;
  final ExactAmount? profit;

  bool get wasRead => profit != null && income != null && expenses != null;

  String get name => monthNames[month.month - 1];

  /// Nothing came in and nothing went out: a real answer, and different from an unread month.
  bool get wasQuiet => wasRead && income!.isZero && expenses!.isZero;

  /// Built from the engine's answer for that month: the lines are the figures, and net income is the profit.
  factory MonthFigure.of(DateTime month, IncomeStatement statement) =>
      MonthFigure(
        month: month,
        income: _total(statement.revenues),
        expenses: _total(statement.expenses),
        profit: statement.netIncome,
      );

  factory MonthFigure.unread(DateTime month) =>
      MonthFigure(month: month, income: null, expenses: null, profit: null);

  static ExactAmount _total(List<IncomeStatementItem> items) =>
      items.fold(ExactAmount.zero, (running, item) => running + item.amount);
}

/// The months of the year so far, January to the month that holds [today].
///
/// Whole months, because a part month read against whole ones would look like a collapse in trade on the first of the month.
List<DateTime> monthsSoFar(DateTime today) => [
  for (var month = 1; month <= today.month; month++)
    DateTime(today.year, month),
];

/// The last day of [start]'s month, which is what the engine reads as the end of a period.
DateTime endOfMonth(DateTime start) => DateTime(start.year, start.month + 1, 0);

/// A year's months, with what can and cannot be said about them.
class YearTrend {
  const YearTrend(this.months);

  final List<MonthFigure> months;

  List<MonthFigure> get read => months.where((month) => month.wasRead).toList();

  int get unread => months.length - read.length;

  /// Every month answered, which is the only state in which a year total means anything.
  bool get isWhole => unread == 0;

  ExactAmount get income =>
      read.fold(ExactAmount.zero, (running, month) => running + month.income!);

  ExactAmount get expenses => read.fold(
    ExactAmount.zero,
    (running, month) => running + month.expenses!,
  );

  ExactAmount get profit =>
      read.fold(ExactAmount.zero, (running, month) => running + month.profit!);

  /// The strongest month by profit, ignoring months that were not read. Sales months, not calendar months: a quiet February
  /// is not the strongest month just because it is first.
  MonthFigure? get best {
    MonthFigure? leader;
    for (final month in read) {
      if (leader == null || month.profit! > leader.profit!) leader = month;
    }
    return leader;
  }

  /// The weakest month by profit, which is the one worth understanding.
  MonthFigure? get worst {
    MonthFigure? trailing;
    for (final month in read) {
      if (trailing == null || month.profit! < trailing.profit!)
        trailing = month;
    }
    return trailing;
  }

  /// The largest absolute figure in any month, so the bars on screen are all drawn to one scale.
  ExactAmount get largest {
    var largest = ExactAmount.zero;
    for (final month in read) {
      for (final figure in [month.income!, month.expenses!, month.profit!]) {
        final size = figure < ExactAmount.zero ? figure.negated() : figure;
        if (size > largest) largest = size;
      }
    }
    return largest;
  }

  /// Months that lost money, which is the count a person acts on.
  Iterable<MonthFigure> get losing =>
      read.where((month) => month.profit! < ExactAmount.zero);
}
