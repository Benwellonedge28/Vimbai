// Stock that is not what the books say it is.
//
// A count that comes up short means part of what the books claim to own is gone: it was paid for, it sits in the accounts
// as something owned, and the month has to carry the cost of it. A count that comes up over means the business owns more
// than it thought, and the cost carried earlier comes back.
//
// The phone states the difference in money, because that is what the count says. It never multiplies a quantity by a price:
// the engine does not know the business's costing method, and guessing one would put a figure in the books that nobody can
// defend. Which way the count went is the person's word, and the engine keeps the reference so the same count cannot be
// recorded twice.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One thing a count found: what was counted, which way it went, and what the difference comes to.
class StockAdjustment {
  const StockAdjustment({
    required this.entryDate,
    required this.item,
    required this.direction,
    required this.amount,
    required this.reason,
  });

  final String entryDate;
  final String item;

  /// `down` when the books carry more than there is, `up` when there is more than the books carry.
  final String direction;
  final String amount;
  final String reason;

  /// Which way the count went, in words a person reads rather than the engine's.
  String get way =>
      direction == 'up' ? 'more than the books' : 'less than the books';

  factory StockAdjustment.fromJson(Map<String, dynamic> json) =>
      StockAdjustment(
        entryDate: (json['entry_date'] as String?) ?? '',
        item: (json['item'] as String?) ?? '',
        direction: (json['direction'] as String?) ?? 'down',
        amount: (json['amount'] as String?) ?? '',
        reason: (json['reason'] as String?) ?? '',
      );
}

class StockAdjustmentsPostingService extends ChangeNotifier {
  StockAdjustmentsPostingService.fresh({http.Client? client})
    : _client = client;

  static final StockAdjustmentsPostingService instance =
      StockAdjustmentsPostingService.fresh();

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

  /// What counting has found in this Book, newest first, narrowed when asked.
  Future<List<StockAdjustment>> read({
    String item = '',
    String period = '',
  }) async {
    final query = <String, String>{
      if (item.trim().isNotEmpty) 'item': item.trim(),
      if (period.trim().isNotEmpty) 'period': period.trim(),
    };
    final uri = Uri.parse(
      '$_base/accounting/stock-adjustments',
    ).replace(queryParameters: query.isEmpty ? null : query);
    final answer = await _http.get(uri, headers: _headers);
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    final body = jsonDecode(answer.body);
    if (body is! List) return const [];
    return [
      for (final row in body)
        if (row is Map<String, dynamic>) StockAdjustment.fromJson(row),
    ];
  }

  /// Record what a count found: [direction] is `down` for a shortage, `up` for stock found.
  Future<StockAdjustment> post({
    required String entryDate,
    required String item,
    required String direction,
    required String amount,
    String reason = '',
    String notes = '',
  }) async {
    final answer = await _http.post(
      Uri.parse('$_base/accounting/stock-adjustments'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate.trim(),
        'item': item.trim(),
        'direction': direction,
        'amount': amount.trim(),
        if (reason.trim().isNotEmpty) 'reason': reason.trim(),
        if (notes.trim().isNotEmpty) 'notes': notes.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return StockAdjustment.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
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
