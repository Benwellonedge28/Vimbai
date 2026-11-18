// Closing a month: the decision that last month's figures are final.
//
// A set of books is worth nothing the moment last month can still change. Reports are produced, a statement is
// reconciled, a return is filed, and then a late invoice is typed with a date three weeks back and every figure that was
// read quietly becomes a different figure. So a month can be closed, and the engine then refuses anything dated inside
// it.
//
// This file holds the reading of that decision: which months are closed, which month is the one being lived in, and
// whether the month being closed is ready. It does no arithmetic of its own - the closed months come from the engine, and
// whether the books are in order comes from the checks that already own that question.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One month the Book has reported on and locked.
class ClosedPeriod {
  const ClosedPeriod({
    required this.period,
    required this.closesOn,
    required this.closedBy,
    required this.note,
  });

  /// The month, as `2026-11`.
  final String period;

  /// The day the close was made, which is when the decision was taken rather than what it covers.
  final String closesOn;

  final String closedBy;
  final String note;

  factory ClosedPeriod.fromJson(Map<String, dynamic> json) => ClosedPeriod(
    period: (json['period'] as String?) ?? '',
    closesOn: (json['closes_on'] as String?) ?? '',
    closedBy: (json['closed_by'] as String?) ?? '',
    note: (json['note'] as String?) ?? '',
  );

  /// The month as a person says it: "November 2026".
  String get spoken => spokenMonth(period);

  /// "Closed by Samuel on 2026-12-01", and the note when there is one.
  String get trail {
    final parts = <String>[];
    if (closedBy.isNotEmpty) parts.add('Closed by $closedBy');
    if (closesOn.isNotEmpty) {
      parts.add(parts.isEmpty ? 'Closed on $closesOn' : 'on $closesOn');
    }
    final trail = parts.join(' ');
    if (note.isEmpty) return trail;
    return trail.isEmpty ? note : '$trail. $note';
  }
}

const List<String> _monthNames = [
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

/// `2026-11` as "November 2026", and anything unreadable handed back as it came.
String spokenMonth(String period) {
  if (period.length < 7) return period;
  final year = period.substring(0, 4);
  final month = int.tryParse(period.substring(5, 7));
  if (month == null || month < 1 || month > 12) return period;
  return '${_monthNames[month - 1]} $year';
}

/// The month a day falls in: `2026-11-18` reads as `2026-11`.
String monthOf(String day) {
  final trimmed = day.trim();
  if (trimmed.length < 7) return '';
  return trimmed.substring(0, 7);
}

/// The month before `2026-11`, which is `2026-10`.
String monthBefore(String month) {
  if (month.length < 7) return month;
  final year = int.tryParse(month.substring(0, 4));
  final number = int.tryParse(month.substring(5, 7));
  if (year == null || number == null || number < 1 || number > 12) {
    return month;
  }
  if (number == 1) return '${year - 1}-12';
  return '$year-${(number - 1).toString().padLeft(2, '0')}';
}

/// Which month is closed, and what the person is being offered.
///
/// The month on offer is the one just gone, never the one being lived in: closing a month that has not finished would
/// lock out the entries that belong in it, and the engine refuses that too rather than trusting this file alone.
class MonthClose {
  const MonthClose({
    required this.today,
    required this.closedMonths,
    required this.closed,
  });

  /// The day the reading was made, as `2026-11-18`.
  final String today;

  /// Every month this Book has closed, oldest first.
  final List<String> closedMonths;

  /// The same months with who closed them and when.
  final List<ClosedPeriod> closed;

  /// The month being lived in, which cannot be closed yet.
  String get currentMonth => monthOf(today);

  /// The month a person can close now: the one just gone, or the earliest one still open when several were missed.
  String get monthOnOffer {
    final current = currentMonth;
    if (current.isEmpty) return '';
    // the earliest month not yet closed, walking back from the one just gone
    var candidate = monthBefore(current);
    final closedSet = closedMonths.toSet();
    var guard = 0;
    while (closedSet.contains(candidate) && guard < 240) {
      candidate = monthBefore(candidate);
      guard += 1;
    }
    return candidate;
  }

  bool get nothingClosed => closedMonths.isEmpty;

  /// Whether the month on offer is already closed, which happens when the engine's list moved under this screen.
  bool isClosed(String month) => closedMonths.contains(month);

  /// The first day the open books begin, or empty when nothing is closed.
  String get openFrom {
    if (closedMonths.isEmpty) return '';
    final last = closedMonths.reduce((a, b) => a.compareTo(b) >= 0 ? a : b);
    final year = int.tryParse(last.substring(0, 4));
    final number = int.tryParse(last.substring(5, 7));
    if (year == null || number == null) return '';
    if (number == 12) return '${year + 1}-01-01';
    return '$year-${(number + 1).toString().padLeft(2, '0')}-01';
  }

  /// What to say first.
  ///
  /// The month on offer leads, because that is the decision in front of the person. When nothing has been closed the
  /// sentence says what closing does rather than pretending a state exists.
  String get headline {
    if (currentMonth.isEmpty) {
      return 'The books are open';
    }
    if (closedMonths.contains(currentMonth)) {
      return '${spokenMonth(currentMonth)} is closed';
    }
    final onOffer = monthOnOffer;
    if (onOffer.isEmpty) return 'The books are open';
    return '${spokenMonth(onOffer)} is still open';
  }

  /// The sentence under the headline.
  String get detail {
    if (nothingClosed) {
      return 'Closing a month makes its figures final: nothing more can be posted into it, and a correction goes into '
          'the month you are in now.';
    }
    final parts = <String>[];
    parts.add(
      closedMonths.length == 1
          ? '${spokenMonth(closedMonths.first)} is closed'
          : '${closedMonths.length} months are closed, up to ${spokenMonth(closedMonths.last)}',
    );
    if (openFrom.isNotEmpty)
      parts.add('so entries can carry any day from $openFrom');
    return '${parts.join(', ')}.';
  }
}

/// The door that holds the decision.
class ClosedPeriodsService {
  ClosedPeriodsService._();

  static final ClosedPeriodsService instance = ClosedPeriodsService._();

  /// A second instance for a test that needs a transport of its own.
  ClosedPeriodsService.fresh();

  static const String _base = AppConfig.accountingRoute;

  http.Client _client = http.Client();

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

  /// The months this Book has closed, oldest first.
  Future<List<String>> closedMonths() async {
    final answer = await _client.get(
      Uri.parse('$_base/closed-periods'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    final body = jsonDecode(answer.body) as Map<String, dynamic>;
    return [
      for (final month in (body['closed_months'] as List?) ?? const [])
        month.toString(),
    ];
  }

  /// Everything the engine holds about the months closed.
  Future<MonthClose> read(String today) async {
    final answer = await _client.get(
      Uri.parse('$_base/closed-periods'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    final body = jsonDecode(answer.body) as Map<String, dynamic>;
    final months = [
      for (final month in (body['closed_months'] as List?) ?? const [])
        month.toString(),
    ];
    return MonthClose(
      today: today,
      closedMonths: months,
      closed: [
        for (final row in (body['closed'] as List?) ?? const [])
          ClosedPeriod.fromJson(row as Map<String, dynamic>),
      ],
    );
  }

  /// Close a month. The engine decides whether it may be closed, and says why not when it may not.
  Future<ClosedPeriod> close({
    required String period,
    required String closesOn,
    required String closedBy,
    String note = '',
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/closed-periods'),
      headers: _headers,
      body: jsonEncode({
        'period': period,
        'closes_on': closesOn,
        'closed_by': closedBy,
        'note': note,
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      // the engine's own sentence is the useful one: it names the month and the way out
      throw Exception(_reason(answer.body, answer.statusCode));
    }
    return ClosedPeriod.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// The engine's refusal in the engine's words, without the transport noise around it.
  String _reason(String body, int status) {
    try {
      final decoded = jsonDecode(body);
      if (decoded is Map<String, dynamic>) {
        for (final key in ['error', 'detail', 'message']) {
          final value = decoded[key];
          if (value is String && value.isNotEmpty) return value;
        }
      }
    } catch (_) {
      // a body that is not JSON is reported as it came
    }
    return 'The engine answered $status: $body';
  }
}
