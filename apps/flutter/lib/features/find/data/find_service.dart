// Finding one thing in a Book.
//
// A Book kept for a year holds thousands of entries, and until now the only way to reach one was to remember which screen it
// was on: a receipt was in the cash book, an invoice in the invoices list, a supplier's bill in what you owe. A person who
// remembers "INV-2" and not which door it went through had nowhere to type it.
//
// The engine searches and answers; this carries the words and repeats the answer, saying for each hit what it is and where
// it opens. Nothing is matched here: a phone that decided what matched would disagree with the screen it opens.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One thing the search found, and where it opens.
class FoundThing {
  const FoundThing({
    required this.kind,
    required this.id,
    required this.title,
    required this.subtitle,
    required this.matchedOn,
    required this.status,
  });

  /// `entry` or `account`.
  final String kind;

  /// The entry's id, or the account's number.
  final String id;
  final String title;
  final String subtitle;

  /// Which part of it the words were found in: reference, description, source, name or number.
  final String matchedOn;

  /// For an entry: `posted`, `draft`, or whatever the engine said.
  final String status;

  /// Whether this is an entry that has not been posted yet.
  bool get isNotPosted => kind == 'entry' && status != 'posted';

  factory FoundThing.fromJson(Map<String, dynamic> json) => FoundThing(
    kind: (json['kind'] as String?) ?? '',
    id: (json['id'] as String?) ?? '',
    title: (json['title'] as String?) ?? '',
    subtitle: (json['subtitle'] as String?) ?? '',
    matchedOn: (json['matched_on'] as String?) ?? '',
    status: (json['status'] as String?) ?? '',
  );
}

/// What a search answered.
class FindAnswer {
  const FindAnswer({
    required this.query,
    required this.entries,
    required this.accounts,
  });

  final String query;
  final List<FoundThing> entries;
  final List<FoundThing> accounts;

  int get count => entries.length + accounts.length;

  /// Whether the search was asked and found nothing.
  bool get foundNothing => count == 0;

  factory FindAnswer.fromJson(Map<String, dynamic> json) => FindAnswer(
    query: (json['query'] as String?) ?? '',
    entries: [
      for (final row in (json['entries'] as List?) ?? const [])
        if (row is Map<String, dynamic>) FoundThing.fromJson(row),
    ],
    accounts: [
      for (final row in (json['accounts'] as List?) ?? const [])
        if (row is Map<String, dynamic>) FoundThing.fromJson(row),
    ],
  );
}

class FindService extends ChangeNotifier {
  FindService.fresh({http.Client? client}) : _client = client;

  static final FindService instance = FindService.fresh();

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

  /// Everything in this Book that holds [words], in its entries and its accounts.
  Future<FindAnswer> find(String words) async {
    final uri = Uri.parse(
      '$_base/accounting/find',
    ).replace(queryParameters: {'q': words.trim()});
    final answer = await _http.get(uri, headers: _headers);
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    return FindAnswer.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
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
