// A Book's spending plan, and what was actually taken up against it.
//
// A plan says what each account is meant to take up over a period. The report says what the posted ledger actually took
// up. Both figures come from the engine: the plan because it is kept with the Book, and the uptake because exactly one
// place decides what "spent" means and that place is the ledger, not a phone adding journal lines up.
//
// So nothing here adds money together. This file reads what came back, names each line's state in words, and turns the
// whole answer into the one sentence a person needs first: is this Book inside its plan, and by how much.
import 'package:vimbai_mobile_client/core/api/amount.dart';

/// Where one account stands against its line of the plan.
enum LineState {
  /// It has taken up less than the plan allows, so there is room left.
  under,

  /// It has taken up exactly what was planned.
  on,

  /// It has gone past its line.
  over;

  /// The state the engine recorded, or [under] for one this app does not know: an unrecognised state is never shown as
  /// "over", because a false alarm sends a person looking for a problem that is not there.
  static LineState fromWire(String? wire) {
    for (final state in LineState.values) {
      if (state.name == wire) return state;
    }
    return LineState.under;
  }
}

/// Whether the Book as a whole is inside its plan.
enum PlanVerdict {
  /// Every account is at or under its line.
  within,

  /// At least one account has gone past its line.
  over;

  static PlanVerdict fromWire(String? wire) =>
      wire == 'over' ? PlanVerdict.over : PlanVerdict.within;
}

/// One line of the plan as it was written: what an account is meant to take up, and what for.
class PlanLine {
  const PlanLine({
    required this.category,
    required this.accountNumber,
    required this.budgeted,
  });

  /// What the line is for, in a person's words: "Rent", "Fuel".
  final String category;

  /// The account in the Book's own chart the line plans for.
  final String accountNumber;

  /// What the line allows over the plan's period.
  final ExactAmount budgeted;

  factory PlanLine.fromJson(Map<String, dynamic> json) => PlanLine(
    category: (json['category'] as String?) ?? '',
    accountNumber: (json['account_number'] as String?) ?? '',
    budgeted:
        ExactAmount.tryParse('${json['budgeted_amount'] ?? '0'}') ??
        ExactAmount.zero,
  );

  Map<String, dynamic> toJson() => {
    'category': category,
    'account_number': accountNumber,
    'budgeted_amount': budgeted.toLedgerString(),
  };
}

/// A plan as the Book keeps it.
class SpendingPlan {
  const SpendingPlan({
    required this.id,
    required this.name,
    required this.startDate,
    required this.endDate,
    required this.currency,
    required this.lines,
    required this.totalBudgeted,
  });

  final String id;
  final String name;

  /// The first day the plan covers, as `YYYY-MM-DD`.
  final String startDate;

  /// The last day the plan covers, as `YYYY-MM-DD`.
  final String endDate;

  final String currency;

  /// Every account the plan names, in the order it was written.
  final List<PlanLine> lines;

  /// What the plan comes to, as the engine added it up.
  final ExactAmount totalBudgeted;

  int get lineCount => lines.length;

  bool get isEmpty => lines.isEmpty;

  factory SpendingPlan.fromJson(Map<String, dynamic> json) => SpendingPlan(
    id: (json['id'] as String?) ?? '',
    name: (json['name'] as String?) ?? '',
    startDate: (json['start_date'] as String?) ?? '',
    endDate: (json['end_date'] as String?) ?? '',
    currency: (json['currency'] as String?) ?? '',
    lines: [
      for (final row in (json['lines'] as List?) ?? const [])
        PlanLine.fromJson(row as Map<String, dynamic>),
    ],
    totalBudgeted:
        ExactAmount.tryParse('${json['total_budgeted'] ?? '0'}') ??
        ExactAmount.zero,
  );
}

/// One account's line against what was actually taken up.
class VarianceLine {
  const VarianceLine({
    required this.category,
    required this.accountNumber,
    required this.budgeted,
    required this.againstPlan,
    required this.remaining,
    required this.state,
  });

  final String category;
  final String accountNumber;

  /// What the plan allows for this account.
  final ExactAmount budgeted;

  /// What the posted ledger says the account has taken up, signed so a positive figure is spending.
  final ExactAmount againstPlan;

  /// What is left of the line. Below nothing means the account has gone past it.
  final ExactAmount remaining;

  final LineState state;

  bool get isOver => state == LineState.over;

  /// How far past the line this account has gone, or nothing when it has not.
  ExactAmount get pastBy =>
      isOver ? ExactAmount.zero - remaining : ExactAmount.zero;

  factory VarianceLine.fromJson(Map<String, dynamic> json) => VarianceLine(
    category: (json['category'] as String?) ?? '',
    accountNumber: (json['account_number'] as String?) ?? '',
    budgeted:
        ExactAmount.tryParse('${json['budgeted_amount'] ?? '0'}') ??
        ExactAmount.zero,
    againstPlan:
        ExactAmount.tryParse('${json['against_plan_amount'] ?? '0'}') ??
        ExactAmount.zero,
    remaining:
        ExactAmount.tryParse('${json['remaining_amount'] ?? '0'}') ??
        ExactAmount.zero,
    state: LineState.fromWire(json['state'] as String?),
  );
}

/// A plan against the ledger: the whole answer one screen needs.
class PlanVariance {
  const PlanVariance({
    required this.id,
    required this.name,
    required this.startDate,
    required this.endDate,
    required this.currency,
    required this.totalBudgeted,
    required this.totalAgainstPlan,
    required this.remaining,
    required this.verdict,
    required this.accountsOver,
    required this.worstAccount,
    required this.worstPastBy,
    required this.lines,
  });

  final String id;
  final String name;
  final String startDate;
  final String endDate;
  final String currency;

  /// What the whole plan allows.
  final ExactAmount totalBudgeted;

  /// What the ledger says the planned accounts have taken up between the plan's own dates.
  final ExactAmount totalAgainstPlan;

  /// What is left over the plan as a whole. Below nothing means the Book has gone past it.
  final ExactAmount remaining;

  final PlanVerdict verdict;

  /// How many accounts have gone past their line.
  final int accountsOver;

  /// The account furthest past its line, when any has gone past.
  final String? worstAccount;

  /// How far that account has gone past its line.
  final ExactAmount? worstPastBy;

  /// Every line, the account furthest past its line first, as the engine ordered them.
  final List<VarianceLine> lines;

  bool get isOver => verdict == PlanVerdict.over;

  /// How far past the plan the Book as a whole has gone, or nothing when it is inside it.
  ExactAmount get pastBy =>
      remaining.isNegative ? ExactAmount.zero - remaining : ExactAmount.zero;

  /// The one sentence to lead with.
  ///
  /// It says the state first and the figure second, because a person reading a budget screen wants to know whether to
  /// worry before they want a number. A Book that has gone past its plan names the account to look at, so the sentence
  /// ends with somewhere to go rather than a figure to interpret.
  String get headline {
    if (isOver) {
      final worst = worstAccount;
      final past = worstPastBy;
      if (worst != null && past != null) {
        final named = _named(worst);
        return 'Over plan by ${past.toDecimalString()} on $named.';
      }
      return 'Over plan by ${pastBy.toDecimalString()}.';
    }
    return '${remaining.toDecimalString()} left of '
        '${totalBudgeted.toDecimalString()} planned.';
  }

  /// The account's own name where the plan gave it one, so a person reads "Office costs" and not "6100".
  String _named(String accountNumber) {
    for (final line in lines) {
      if (line.accountNumber == accountNumber &&
          line.category.trim().isNotEmpty) {
        return line.category;
      }
    }
    return accountNumber;
  }

  factory PlanVariance.fromJson(Map<String, dynamic> json) {
    final worst = json['worst_account'] as Map<String, dynamic>?;
    return PlanVariance(
      id: (json['id'] as String?) ?? '',
      name: (json['name'] as String?) ?? '',
      startDate: (json['start_date'] as String?) ?? '',
      endDate: (json['end_date'] as String?) ?? '',
      currency: (json['currency'] as String?) ?? '',
      totalBudgeted:
          ExactAmount.tryParse('${json['total_budgeted'] ?? '0'}') ??
          ExactAmount.zero,
      totalAgainstPlan:
          ExactAmount.tryParse('${json['total_against_plan'] ?? '0'}') ??
          ExactAmount.zero,
      remaining:
          ExactAmount.tryParse('${json['remaining_amount'] ?? '0'}') ??
          ExactAmount.zero,
      verdict: PlanVerdict.fromWire(json['verdict'] as String?),
      accountsOver: (json['accounts_over'] as num?)?.toInt() ?? 0,
      worstAccount: worst?['account_number'] as String?,
      worstPastBy: worst == null
          ? null
          : ExactAmount.tryParse('${worst['past_by'] ?? '0'}'),
      lines: [
        for (final row in (json['lines'] as List?) ?? const [])
          VarianceLine.fromJson(row as Map<String, dynamic>),
      ],
    );
  }
}

/// True when a plan already names [accountNumber], so the same account is not planned for twice.
///
/// A second line for one account would count its plan twice while the ledger counts its spending once, so the engine
/// refuses it; this lets the form say so before anything is sent.
bool accountAlreadyPlanned(List<PlanLine> lines, String accountNumber) {
  for (final line in lines) {
    if (line.accountNumber == accountNumber) return true;
  }
  return false;
}
