// A fact a person states, posted by the engine.
//
// The journal screens asked a person to choose two accounts, which is the bookkeeper's job and needs a chart they never
// wrote. The engine now derives the posting from what happened: money spent on something, or money received for
// something. The words the person uses are matched against the Book's own chart there, and a refusal comes back naming
// what the Book holds, so nothing here knows the name of an account.
//
// Rehearsing is the same question asked without posting, so the screen can show the entry before it is made. Vimbai does
// not move money: it records what already moved.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// The two facts a person can state.
enum FactKind {
  /// Money left the business on something.
  spent('spent'),

  /// Money came into the business for something.
  received('received');

  const FactKind(this.wire);

  /// What the engine calls it.
  final String wire;
}

/// One side of the posting the engine derived.
class FactSide {
  const FactSide({required this.number, required this.name});

  final String number;
  final String name;

  /// The account as a person reads it: its name, with its number when the Book gave it one.
  String get label =>
      name.isEmpty ? number : (number.isEmpty ? name : '$name ($number)');

  factory FactSide.fromJson(Object? json) {
    if (json is Map<String, dynamic>) {
      return FactSide(
        number: (json['number'] as String?) ?? '',
        name: (json['name'] as String?) ?? '',
      );
    }
    return const FactSide(number: '', name: '');
  }
}

/// The answer to a stated fact: what the engine would post, or what it did post.
class FactAnswer {
  const FactAnswer({
    required this.posted,
    required this.kind,
    required this.entryDate,
    required this.debit,
    required this.credit,
    required this.amount,
    required this.description,
    required this.entryId,
  });

  /// False for a rehearsal, which answers the posting without making it.
  final bool posted;

  final FactKind kind;
  final String entryDate;

  /// The account that takes the value.
  final FactSide debit;

  /// The account that gives it up.
  final FactSide credit;

  /// The amount as the engine states it.
  final String amount;

  /// What the entry says, in words.
  final String description;

  /// Empty for a rehearsal.
  final String entryId;

  factory FactAnswer.fromJson(Map<String, dynamic> json, FactKind asked) {
    final posting = json['posting'] ?? json['would_post'];
    final body = posting is Map<String, dynamic>
        ? posting
        : const <String, dynamic>{};
    final said = (json['what'] as String?) ?? asked.wire;
    return FactAnswer(
      posted: json['posted'] == true,
      kind: FactKind.values.firstWhere(
        (kind) => kind.wire == said,
        orElse: () => asked,
      ),
      entryDate: (json['entry_date'] as String?) ?? '',
      debit: FactSide.fromJson(body['debit']),
      credit: FactSide.fromJson(body['credit']),
      amount: (body['amount'] as String?) ?? '',
      description: (body['description'] as String?) ?? '',
      entryId: (json['entry_id'] as String?) ?? '',
    );
  }
}

/// Stating facts to the engine: money spent on something, money received for something.
class FactPostingService {
  FactPostingService._();

  static final FactPostingService instance = FactPostingService._();

  /// A second instance for a test that needs a transport of its own.
  FactPostingService.fresh();

  static const String _base = AppConfig.apiUrl;

  http.Client _client = http.Client();

  /// A test calls the service against a transport it controls. Nothing in the app sets this.
  @visibleForTesting
  void useClient(http.Client client) => _client = client;

  String? _token;

  /// A test may act as a particular session; the app leaves this alone so the session's own token is used.
  @visibleForTesting
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// What the engine would post for [kind] of [amount] on [entryDate], without posting it.
  Future<FactAnswer> rehearse({
    required FactKind kind,
    required String entryDate,
    required ExactAmount amount,
    required String account,
    String money = '',
    String note = '',
  }) => _state(
    kind: kind,
    entryDate: entryDate,
    amount: amount,
    account: account,
    money: money,
    note: note,
    rehearse: true,
  );

  /// Record [kind] of [amount] on [entryDate] and return the entry the engine posted.
  Future<FactAnswer> post({
    required FactKind kind,
    required String entryDate,
    required ExactAmount amount,
    required String account,
    String money = '',
    String note = '',
  }) => _state(
    kind: kind,
    entryDate: entryDate,
    amount: amount,
    account: account,
    money: money,
    note: note,
    rehearse: false,
  );

  Future<FactAnswer> _state({
    required FactKind kind,
    required String entryDate,
    required ExactAmount amount,
    required String account,
    required String money,
    required String note,
    required bool rehearse,
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/facts'),
      headers: _headers,
      body: jsonEncode({
        'what': kind.wire,
        'entry_date': entryDate.trim(),
        'amount': amount.toDecimalString(),
        'account': account.trim(),
        if (money.trim().isNotEmpty) 'money': money.trim(),
        if (note.trim().isNotEmpty) 'note': note.trim(),
        if (rehearse) 'rehearse': true,
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return FactAnswer.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
      kind,
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
    return 'The engine refused the fact: ${answer.statusCode}.';
  }
}
