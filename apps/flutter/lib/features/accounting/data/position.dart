// What the business is worth, read from the engine's balance sheet.
//
// The engine answers every asset, liability and equity account with its balance, and the three totals plus liabilities and
// equity added together. Three things this reading adds, all of them honesty rather than arithmetic:
//
//   * The totals are the engine's, not re-added here. It already added them, and two additions of one set of figures is one
//     place too many for them to disagree.
//   * A section lists the accounts that carry a balance. A twelve-account starter chart would otherwise bury the three
//     accounts that mean anything under nine zeros.
//   * "Balanced" is not claimed for a Book with nothing in it. A fresh Book is trivially balanced, and saying so would read as
//     a check that passed when there was nothing to check.
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';

/// One section of the sheet: the accounts carrying a balance, and the engine's own total for the section.
class PositionSection {
  const PositionSection({
    required this.title,
    required this.carrying,
    required this.silent,
    required this.total,
  });

  final String title;

  /// The accounts with something in them, largest first.
  final List<BalanceSheetItem> carrying;

  /// How many accounts of this kind hold nothing. Counted rather than listed, so the section stays readable.
  final int silent;

  final ExactAmount total;

  bool get isEmpty => carrying.isEmpty;
}

/// How the sheet came out.
enum PositionVerdict {
  /// Assets equal liabilities plus equity, and there is something on the sheet.
  balances,

  /// They do not, by [Position.gap].
  outOfBalance,

  /// Nothing has been recorded yet: every section is empty, so there is nothing to prove.
  nothingRecorded,
}

/// The sheet as a person reads it.
class Position {
  const Position({
    required this.asOfDate,
    required this.assets,
    required this.liabilities,
    required this.equity,
    required this.verdict,
    required this.gap,
  });

  final DateTime asOfDate;
  final PositionSection assets;
  final PositionSection liabilities;
  final PositionSection equity;
  final PositionVerdict verdict;

  /// How far apart the two sides are, when they are. Zero when the sheet balances.
  final ExactAmount gap;

  /// The sentence the screen leads with.
  String get headline {
    switch (verdict) {
      case PositionVerdict.balances:
        return 'The two sides agree: ${assets.total.toDecimalString()} of '
            'assets, and ${liabilities.total.toDecimalString()} owed plus '
            '${equity.total.toDecimalString()} of the owners\' stake.';
      case PositionVerdict.outOfBalance:
        return 'The two sides are out by ${gap.toDecimalString()}: assets say '
            '${assets.total.toDecimalString()}, and liabilities with equity say '
            '${_liabilitiesAndEquity.toDecimalString()}.';
      case PositionVerdict.nothingRecorded:
        return 'Nothing is on the sheet yet. Post an entry and the business\'s '
            'assets, what it owes and the owners\' stake will appear here.';
    }
  }

  ExactAmount get _liabilitiesAndEquity => liabilities.total + equity.total;
}

/// Read the engine's answer as a person would.
Position positionOf(BalanceSheet sheet) {
  PositionSection section(
    String title,
    List<BalanceSheetItem> rows,
    ExactAmount total,
  ) {
    final carrying = [
      for (final row in rows)
        if (!row.amount.isZero) row,
    ];
    carrying.sort((a, b) => b.amount.compareTo(a.amount));
    return PositionSection(
      title: title,
      carrying: carrying,
      silent: rows.length - carrying.length,
      total: total,
    );
  }

  final assets = section('Assets', sheet.assets, sheet.totalAssets);
  final liabilities = section(
    'Liabilities',
    sheet.liabilities,
    sheet.totalLiabilities,
  );
  final equity = section('Equity', sheet.equity, sheet.totalEquity);

  final difference = sheet.totalAssets - sheet.totalLiabilitiesEquity;
  // the size of the disagreement, whichever side is ahead, without a unary minus on exact amounts
  final gap = difference.isZero
      ? ExactAmount.zero
      : (difference.negative
            ? sheet.totalLiabilitiesEquity - sheet.totalAssets
            : difference);
  final nothing =
      assets.isEmpty &&
      liabilities.isEmpty &&
      equity.isEmpty &&
      sheet.totalAssets.isZero &&
      sheet.totalLiabilitiesEquity.isZero;

  return Position(
    asOfDate: sheet.asOfDate,
    assets: assets,
    liabilities: liabilities,
    equity: equity,
    verdict: nothing
        ? PositionVerdict.nothingRecorded
        : (gap.isZero
              ? PositionVerdict.balances
              : PositionVerdict.outOfBalance),
    gap: gap,
  );
}
