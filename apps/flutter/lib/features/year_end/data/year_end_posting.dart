// Closing the year: the year's profit moved to one place, and its trading accounts left at nothing.
//
// A year's sales and costs have done their work once the year is over. What they came to is the year's profit, and the
// accounts that carried them should not go on carrying them into the next year: a reader of next February's sales wants
// next February's sales, not last year's total sitting behind them.
//
// The engine decides the figures, from what was posted. This only carries the year and repeats the engine's answer: what
// the year earned, what it spent, what it kept, and the day the closing entry was dated - the first day of the year after,
// because a closed month is never written into and the year's own reports should go on showing what it earned.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// A year this Book has closed, as the engine stated it.
class ClosedYear {
  const ClosedYear({
    required this.year,
    required this.entryDate,
    required this.earned,
    required this.spent,
    required this.profit,
    required this.monthsReopenedSince,
  });

  final String year;

  /// The day the closing entry was dated: the first day of the year after.
  final String entryDate;

  /// What the year's sales and other revenue came to.
  final String earned;

  /// What the year's costs came to.
  final String spent;

  /// What it kept, signed: a negative figure is a loss.
  final String profit;

  /// Months of that year that have since been reopened, so the closing entry no longer describes the year.
  final List<String> monthsReopenedSince;

  /// Whether the year made money rather than losing it.
  bool get madeMoney => !profit.trim().startsWith('-');

  /// What the year kept, said as a person would say it.
  String get whatItKept =>
      madeMoney ? 'kept $profit' : 'lost ${profit.replaceFirst('-', '')}';

  factory ClosedYear.fromJson(Map<String, dynamic> json) => ClosedYear(
    year: (json['year'] as String?) ?? '',
    entryDate: (json['entry_date'] as String?) ?? '',
    earned: (json['earned'] as String?) ?? '',
    spent: (json['spent'] as String?) ?? '',
    profit: (json['profit'] as String?) ?? '',
    monthsReopenedSince: [
      for (final month in (json['months_reopened_since'] as List?) ?? const [])
        if (month is String) month,
    ],
  );
}

class YearEndPostingService extends ChangeNotifier {
  YearEndPostingService.fresh({http.Client? client}) : _client = client;

  static final YearEndPostingService instance = YearEndPostingService.fresh();

  http.Client? _client;

  /// A test replaces the transport, so nothing here needs a server.
  void useClient(http.Client client) => _client = client;

  http.Client get _http => _client ?? http.Client();

  static const String _base = AppConfig.apiUrl;

  /// The token a person signed in with, when the service is used outside the app's own session.
  String? _token;

  /// A test, or a screen holding its own session, states whose books these are.
  void useToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// The years this Book has closed, newest first.
  Future<List<ClosedYear>> read({String year = ''}) async {
    final uri = Uri.parse('$_base/accounting/year-end-close').replace(
      queryParameters: year.trim().isEmpty ? null : {'year': year.trim()},
    );
    final answer = await _http.get(uri, headers: _headers);
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    final body = jsonDecode(answer.body);
    if (body is! List) return const [];
    return [
      for (final row in body)
        if (row is Map<String, dynamic>) ClosedYear.fromJson(row),
    ];
  }

  /// Close [year]. The engine refuses a year whose months are not all closed, and refuses a second close.
  Future<ClosedYear> close({required String year, String notes = ''}) async {
    final answer = await _http.post(
      Uri.parse('$_base/accounting/year-end-close'),
      headers: _headers,
      body: jsonEncode({
        'year': year.trim(),
        if (notes.trim().isNotEmpty) 'notes': notes.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return ClosedYear.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }

  /// The engine's own sentence when it refuses, rather than a status code and a wall of JSON.
  String _refusalOf(http.Response answer) {
    try {
      final body = jsonDecode(answer.body);
      if (body is Map<String, dynamic>) {
        for (final key in ['detail', 'message', 'error']) {
          final words = body[key];
          if (words is String && words.trim().isNotEmpty) return words;
        }
      }
    } catch (_) {
      // the body was not the engine's envelope, so the status is all there is to say
    }
    return 'The engine refused this: ${answer.statusCode}.';
  }
}
