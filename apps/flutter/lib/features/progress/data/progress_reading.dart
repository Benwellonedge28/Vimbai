// How the business is doing, to any day.
//
// A month is how figures are locked, not how they are seen. This reads the engine's own progress answer, which is a view
// over whatever window is asked for: the month so far by default, or any span a person names. Nothing is computed here.
// A phone that worked out its own profit would disagree with the ledger it is reporting on, and the disagreement would be
// discovered at the worst possible moment - by the accountant, a month later.
//
// Two things this reading is careful about, because the screen must be:
//
// * it compares like for like. The engine sends the window it compared against, and this carries it, so the screen can
//   say "against 1-20 September" rather than letting a part month look like a whole one.
// * it carries what is not posted. Profit that leaves out unposted entries is provisional, and the note travels with the
//   figure instead of being left to be found later.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One money account the Book keeps, and what it holds.
class MoneyHeld {
  const MoneyHeld({
    required this.name,
    required this.kind,
    required this.balance,
  });

  final String name;
  final String kind;
  final String balance;

  factory MoneyHeld.fromJson(Map<String, dynamic> json) => MoneyHeld(
    name: (json['name'] as String?) ?? '',
    kind: (json['kind'] as String?) ?? '',
    balance: (json['balance'] as String?) ?? '0',
  );
}

/// How the business is doing over a window, as the ledger has it.
class ProgressReading {
  const ProgressReading({
    required this.from,
    required this.to,
    required this.days,
    required this.revenue,
    required this.costs,
    required this.profit,
    required this.marginPercent,
    required this.comparedFrom,
    required this.comparedTo,
    required this.comparedProfit,
    required this.changeProfit,
    required this.changeRevenue,
    required this.cashTotal,
    required this.cashAccounts,
    required this.owedToYou,
    required this.owedByYou,
    required this.taxOwed,
    required this.drafts,
    required this.oldestDraft,
    required this.note,
  });

  final String from;
  final String to;
  final int days;
  final String revenue;
  final String costs;
  final String profit;
  final String marginPercent;
  final String comparedFrom;
  final String comparedTo;
  final String comparedProfit;
  final String changeProfit;
  final String changeRevenue;
  final String cashTotal;
  final List<MoneyHeld> cashAccounts;
  final String owedToYou;
  final String owedByYou;
  final String taxOwed;
  final int drafts;
  final String oldestDraft;
  final String note;

  /// Whether anything has been posted in this window at all.
  bool get hasTraded => _notZero(revenue) || _notZero(costs);

  /// Whether the entries in the window are all posted, so the figures are final.
  bool get isProvisional => drafts > 0;

  /// Whether money came in against what was made: a profitable window with nothing in the bank is the classic way a good
  /// business dies, so the two are shown side by side and never merged.
  bool get madeMoreThanItHolds =>
      _notZero(profit) && cashTotal.trim() != '' && _less(cashTotal, profit);

  /// The window in the words a person reads: "1-20 October", or "5 October - 8 November".
  String get window {
    final opens = _spoken(from);
    final closes = _spoken(to);
    if (from == to) return opens;
    if (from.substring(0, 7) == to.substring(0, 7)) {
      // the same month: say the month once
      return '${_dayOf(from)}-${closes}';
    }
    return '$opens - $closes';
  }

  String get comparedWindow =>
      '${_spoken(comparedFrom)} - ${_spoken(comparedTo)}';

  factory ProgressReading.fromJson(Map<String, dynamic> json) {
    final compared =
        (json['compared_with'] as Map<String, dynamic>?) ??
        const <String, dynamic>{};
    final change =
        (json['change'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
    final cash =
        (json['cash'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
    final tax =
        (json['tax'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
    final notPosted =
        (json['not_posted'] as Map<String, dynamic>?) ??
        const <String, dynamic>{};
    return ProgressReading(
      from: (json['from'] as String?) ?? '',
      to: (json['to'] as String?) ?? '',
      days: (json['days'] as num?)?.toInt() ?? 0,
      revenue: (json['revenue'] as String?) ?? '0',
      costs: (json['costs'] as String?) ?? '0',
      profit: (json['profit'] as String?) ?? '0',
      marginPercent: (json['margin_percent'] as String?) ?? '0',
      comparedFrom: (compared['from'] as String?) ?? '',
      comparedTo: (compared['to'] as String?) ?? '',
      comparedProfit: (compared['profit'] as String?) ?? '0',
      changeProfit: (change['profit'] as String?) ?? '0',
      changeRevenue: (change['revenue'] as String?) ?? '0',
      cashTotal: (cash['total'] as String?) ?? '0',
      cashAccounts: [
        for (final row
            in (cash['accounts'] as List<dynamic>? ?? const <dynamic>[]))
          MoneyHeld.fromJson(row as Map<String, dynamic>),
      ],
      owedToYou: (json['owed_to_you'] as String?) ?? '0',
      owedByYou: (json['owed_by_you'] as String?) ?? '0',
      taxOwed: (tax['owed'] as String?) ?? '0',
      drafts: (notPosted['drafts'] as num?)?.toInt() ?? 0,
      oldestDraft: (notPosted['oldest'] as String?) ?? '',
      note: (json['note'] as String?) ?? '',
    );
  }
}

const _months = [
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

/// A day as a person says it: "20 October 2026", or the day alone when the month is already on the screen.
String _spoken(String day) {
  final parts = day.split('-');
  if (parts.length != 3) return day;
  final month = int.tryParse(parts[1]);
  final dayOfMonth = int.tryParse(parts[2]);
  if (month == null || dayOfMonth == null || month < 1 || month > 12) {
    return day;
  }
  return '$dayOfMonth ${_months[month - 1]} ${parts[0]}';
}

String _dayOf(String day) {
  final parts = day.split('-');
  return parts.length == 3 ? (int.tryParse(parts[2]) ?? 0).toString() : day;
}

bool _notZero(String amount) {
  final digits = amount.replaceAll(RegExp(r'[^0-9]'), '');
  return digits.split('').any((one) => one != '0');
}

/// Whether [one] is smaller than [two], compared as whole cents so nothing is lost to a double.
bool _less(String one, String two) => _cents(one) < _cents(two);

int _cents(String amount) {
  final text = amount.trim();
  final negative = text.startsWith('-');
  final parts = text.replaceAll('-', '').split('.');
  final whole = int.tryParse(parts.first.isEmpty ? '0' : parts.first) ?? 0;
  final fraction = parts.length > 1 ? parts[1] : '';
  final cents = fraction.padRight(2, '0').substring(0, 2);
  final value = whole * 100 + (int.tryParse(cents) ?? 0);
  return negative ? -value : value;
}

/// Reads how the business is doing from the engine.
class ProgressService {
  ProgressService({http.Client? client}) : _client = client ?? http.Client();

  /// A service whose client is replaced by a test.
  @visibleForTesting
  factory ProgressService.fresh() => ProgressService();

  static const String _base = AppConfig.accountingRoute;

  http.Client _client;

  @visibleForTesting
  void useClient(http.Client client) => _client = client;

  String? _token;

  @visibleForTesting
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// How the business is doing to [to], over [from] to [to], or the month so far when [from] is left out.
  Future<ProgressReading> read({required String to, String from = ''}) async {
    // the query is written out rather than assembled, so the address is one a reader - and the gate that checks every
    // address the app calls - can see whole
    final day = Uri.encodeQueryComponent(to.trim());
    final address = from.trim().isEmpty
        ? '$_base/progress?to=$day'
        : '$_base/progress?to=$day&from=${Uri.encodeQueryComponent(from.trim())}';
    final answer = await _client.get(Uri.parse(address), headers: _headers);
    if (answer.statusCode != 200) {
      throw Exception(_refusalOf(answer));
    }
    return ProgressReading.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// The engine's own words for why it would not answer.
  String _refusalOf(http.Response answer) {
    try {
      final body = jsonDecode(answer.body);
      if (body is Map<String, dynamic>) {
        for (final key in const ['detail', 'message', 'error']) {
          final value = body[key];
          if (value is String && value.trim().isNotEmpty) return value.trim();
        }
      }
    } on FormatException {
      // not JSON: the status is all there is to say
    }
    return 'The books answered ${answer.statusCode}.';
  }
}
