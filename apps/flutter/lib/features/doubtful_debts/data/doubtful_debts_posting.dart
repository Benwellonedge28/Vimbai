// The part of what customers owe that the business does not expect to see.
//
// Writing a debt off is a decision about one customer. A provision is the same worry said in advance and about all of them
// at once: some of what is owed will not come, the business does not yet know which, and a year that reports the whole of
// what is owed as coming reports a profit it may not have made.
//
// The estimate is the person's, so the phone asks what the provision should stand at and sends that figure. The engine
// works out what stands today and posts only the movement, and this repeats the engine's answer: what stood before, which
// way it moved and by how much.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One adjustment to the estimate: what it was raised or lowered to, and what moved.
class ProvisionAdjustment {
  const ProvisionAdjustment({
    required this.entryDate,
    required this.provision,
    required this.previous,
    required this.movement,
    required this.direction,
    required this.reason,
  });

  final String entryDate;

  /// The figure the provision was moved to.
  final String provision;

  /// What stood before it was moved.
  final String previous;

  /// What moved, signed.
  final String movement;

  /// `raised` or `reduced`.
  final String direction;
  final String reason;

  /// Whether this adjustment raised the estimate rather than lowering it.
  bool get raised => direction != 'reduced';

  /// What moved, said without its sign, because the direction says it already.
  String get figure => movement.replaceFirst('-', '');

  factory ProvisionAdjustment.fromJson(Map<String, dynamic> json) =>
      ProvisionAdjustment(
        entryDate: (json['entry_date'] as String?) ?? '',
        provision: (json['provision'] as String?) ?? '',
        previous: (json['previous'] as String?) ?? '',
        movement: (json['movement'] as String?) ?? '',
        direction: (json['direction'] as String?) ?? 'raised',
        reason: (json['reason'] as String?) ?? '',
      );
}

/// What the Book carries against receivables, and the ladder of estimates behind it.
class ProvisionLadder {
  const ProvisionLadder({required this.standing, required this.adjustments});

  /// What the allowance stands at today.
  final String standing;
  final List<ProvisionAdjustment> adjustments;

  /// Whether the Book has never made an estimate.
  bool get isEmpty => adjustments.isEmpty;

  factory ProvisionLadder.fromJson(Map<String, dynamic> json) =>
      ProvisionLadder(
        standing: (json['standing'] as String?) ?? '',
        adjustments: [
          for (final row in (json['adjustments'] as List?) ?? const [])
            if (row is Map<String, dynamic>) ProvisionAdjustment.fromJson(row),
        ],
      );
}

class DoubtfulDebtsPostingService extends ChangeNotifier {
  DoubtfulDebtsPostingService.fresh({http.Client? client}) : _client = client;

  static final DoubtfulDebtsPostingService instance =
      DoubtfulDebtsPostingService.fresh();

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

  /// What stands against receivables today, and every estimate behind it.
  Future<ProvisionLadder> read({String period = ''}) async {
    final uri = Uri.parse('$_base/accounting/doubtful-debts').replace(
      queryParameters: period.trim().isEmpty ? null : {'period': period.trim()},
    );
    final answer = await _http.get(uri, headers: _headers);
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    return ProvisionLadder.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// Move the provision to [provision]. The engine posts only the difference from what stands today.
  Future<ProvisionAdjustment> provide({
    required String entryDate,
    required String provision,
    String reason = '',
    String notes = '',
  }) async {
    final answer = await _http.post(
      Uri.parse('$_base/accounting/doubtful-debts'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate.trim(),
        'provision': provision.trim(),
        if (reason.trim().isNotEmpty) 'reason': reason.trim(),
        if (notes.trim().isNotEmpty) 'notes': notes.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return ProvisionAdjustment.fromJson(
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
