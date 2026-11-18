// Where the money went, read from the engine's cash flow statement.
//
// The engine reads the posted ledger and answers the money that actually moved through the cash and bank accounts, split
// into what the business traded (operating), what it bought or sold to use (investing) and what it borrowed or took from
// its owners (financing). This reading adds no arithmetic to that; it only decides what a person is told first:
//
//   * whether cash grew or shrank, by how much, and from what to what - not a table of every line;
//   * the sources inside each class, largest first, with the ones that moved nothing counted rather than listed;
//   * a plain word when the books and the cash book disagree, because the two are kept by different rules and a person
//     shown one figure will assume it is the other;
//   * the difference between "cash stood still" and "nothing has moved yet" - a fresh Book must not read as a quiet month.
import 'package:vimbai_mobile_client/core/api/amount.dart';

/// One thing the money moved for: the account on the other side of the cash.
class CashSource {
  const CashSource({required this.description, required this.amount});

  final String description;

  /// What it put into (positive) or took out of (negative) the cash.
  final ExactAmount amount;
}

/// One class of movement: operating, investing or financing.
class CashSection {
  const CashSection({
    required this.title,
    required this.sources,
    required this.net,
    required this.quiet,
  });

  final String title;

  /// The sources that moved money, largest first.
  final List<CashSource> sources;

  /// The class's net effect on cash, as the engine counted it.
  final ExactAmount net;

  /// How many counterpart accounts moved nothing in this class. Counted, not listed.
  final int quiet;

  bool get isEmpty => sources.isEmpty;
}

/// How the period came out.
enum MovementVerdict {
  /// More came in than went out.
  grew,

  /// More went out than came in.
  shrank,

  /// Money moved, but as much came in as went out.
  stoodStill,

  /// No money moved at all: nothing has been recorded for this period.
  nothingMoved,
}

/// The statement as a person reads it.
class CashMovement {
  const CashMovement({
    required this.start,
    required this.end,
    required this.sections,
    required this.netChange,
    required this.opening,
    required this.closing,
    required this.unclassified,
    required this.cashBookTotal,
    required this.agreesWithCashBook,
  });

  final DateTime start;
  final DateTime end;
  final List<CashSection> sections;

  /// The period's movement, signed.
  final ExactAmount netChange;

  /// What the business held when the period opened, and when it closed. The engine's figures.
  final ExactAmount opening;
  final ExactAmount closing;

  /// How many counterpart lines the engine could not place in one of the three classes. Reported, never swept in.
  final int unclassified;

  /// What the cash book says is held, and whether it agrees with the statement.
  final ExactAmount cashBookTotal;
  final bool agreesWithCashBook;

  /// The size of the movement, whichever way it went.
  ExactAmount get moved =>
      netChange.isNegative ? ExactAmount.zero - netChange : netChange;

  MovementVerdict get verdict {
    if (netChange.isZero) {
      final moved = sections.any((section) => !section.net.isZero);
      return moved ? MovementVerdict.stoodStill : MovementVerdict.nothingMoved;
    }
    return netChange.isNegative ? MovementVerdict.shrank : MovementVerdict.grew;
  }

  /// The sentence the screen leads with.
  String get headline {
    final from = opening.toDecimalString();
    final to = closing.toDecimalString();
    switch (verdict) {
      case MovementVerdict.grew:
        return 'Cash grew by ${moved.toDecimalString()} over this period, '
            'from $from to $to.';
      case MovementVerdict.shrank:
        return 'Cash fell by ${moved.toDecimalString()} over this period, '
            'from $from to $to.';
      case MovementVerdict.stoodStill:
        return 'Cash stood still over this period: as much came in as went '
            'out, so it is still $to.';
      case MovementVerdict.nothingMoved:
        return 'No money has moved through the cash or bank accounts in this '
            'period.';
    }
  }

  /// Said only when the two records of the same money disagree. The cash book is opened with a balance that was never an
  /// entry, so this is ordinary rather than alarming - but it must be said, not hidden behind one of the two figures.
  String? get cashBookNote {
    if (agreesWithCashBook) return null;
    return 'The books say ${closing.toDecimalString()} is held; the cash book '
        'says ${cashBookTotal.toDecimalString()}.';
  }

  /// Said only when the engine met an account it could not place in one of the three classes.
  String? get unclassifiedNote {
    if (unclassified == 0) return null;
    return '$unclassified line${unclassified == 1 ? '' : 's'} moved money '
        'against an account type this statement does not classify, so '
        '${unclassified == 1 ? 'it is' : 'they are'} not in the three classes '
        'below.';
  }

  /// Read the engine's answer as a person would.
  factory CashMovement.fromJson(Map<String, dynamic> json) {
    final sections = <CashSection>[];
    for (final raw in (json['sections'] as List<dynamic>? ?? const [])) {
      final section = raw as Map<String, dynamic>;
      final sources = <CashSource>[
        for (final line in (section['lines'] as List<dynamic>? ?? const []))
          CashSource(
            description:
                (line as Map<String, dynamic>)['description'] as String? ?? '',
            amount: ExactAmount.fromJson(line['amount']),
          ),
      ];
      sources.sort((a, b) {
        final bySize = b.amount.compareTo(a.amount);
        return bySize != 0 ? bySize : a.description.compareTo(b.description);
      });
      sections.add(
        CashSection(
          title: section['title'] as String? ?? '',
          sources: sources,
          net: ExactAmount.fromJson(section['net']),
          quiet: (section['quiet'] as num?)?.toInt() ?? 0,
        ),
      );
    }
    return CashMovement(
      start:
          DateTime.tryParse(json['start_date'] as String? ?? '') ??
          DateTime.now(),
      end:
          DateTime.tryParse(json['end_date'] as String? ?? '') ??
          DateTime.now(),
      sections: sections,
      netChange: ExactAmount.fromJson(json['net_change']),
      opening: ExactAmount.fromJson(json['opening_cash']),
      closing: ExactAmount.fromJson(json['closing_cash']),
      unclassified: (json['unclassified'] as num?)?.toInt() ?? 0,
      cashBookTotal: ExactAmount.fromJson(json['cash_book_total']),
      agreesWithCashBook: json['agrees_with_cash_book'] as bool? ?? false,
    );
  }
}
