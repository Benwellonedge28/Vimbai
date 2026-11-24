// The wear on what a business owns, reaching the books through the engine's depreciation journal.
//
// A vehicle, a fridge, a laptop, a machine: everything a business keeps rather than sells loses value as it is used, and
// that loss is a cost of trading. It is a cost with no money behind it - nothing leaves the bank in the month the fridge
// ages - so it cannot go through the cash book, and it is not a bill anybody will send.
//
// The asset register already works the figure out, month by month, when a month is charged there. What this talks to is the
// other half: the engine takes the rows the register recorded and posts them, one entry per asset, so the cost reaches the
// books and the thing stays in the books at what it cost with the wear kept beside it.
//
// The screen never computes a charge. Two arithmetics for one figure means that on the day they disagree nobody can say
// which one the books meant, so the figure is the register's and this only carries the month.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One asset's month of wear, as the register recorded it and the engine sees it.
class DepreciationCharge {
  const DepreciationCharge({
    required this.assetName,
    required this.assetId,
    required this.period,
    required this.amount,
    required this.journalEntryId,
  });

  /// What the register calls the thing, which is how a person knows which charge this is.
  final String assetName;
  final String assetId;

  /// The month, as `2026-11`.
  final String period;

  /// What that month's wear was, as the register worked it out.
  final String amount;

  /// The entry that posted it. Empty while it is still only in the register.
  final String journalEntryId;

  bool get isPosted => journalEntryId.isNotEmpty;

  /// The name to show: the register's name, or the asset's id when the register has since been emptied of it.
  String get called => assetName.trim().isNotEmpty ? assetName : assetId;

  factory DepreciationCharge.fromJson(Map<String, dynamic> json) =>
      DepreciationCharge(
        assetName: (json['asset_name'] as String?) ?? '',
        assetId: (json['asset_id'] as String?) ?? '',
        period: (json['period'] as String?) ?? '',
        amount: (json['depreciation_amount'] as String?) ?? '',
        journalEntryId: (json['journal_entry_id'] as String?) ?? '',
      );
}

/// A month posted: what went to the books and what was already there.
class DepreciationPosted {
  const DepreciationPosted({
    required this.period,
    required this.total,
    required this.charged,
    required this.alreadyPosted,
  });

  final String period;

  /// What the month's wear came to, in the Book's own money.
  final String total;

  final List<DepreciationCharge> charged;

  /// Rows that were already in the books. A month is posted once, so these are why a second run changed nothing.
  final List<DepreciationCharge> alreadyPosted;

  factory DepreciationPosted.fromJson(Map<String, dynamic> json) =>
      DepreciationPosted(
        period: (json['period'] as String?) ?? '',
        total: (json['total'] as String?) ?? '',
        charged: ((json['posted'] as List?) ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(DepreciationCharge.fromJson)
            .toList(),
        alreadyPosted: ((json['already_posted'] as List?) ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(DepreciationCharge.fromJson)
            .toList(),
      );
}

/// What the register has recorded and the books have not.
class DepreciationWaiting {
  const DepreciationWaiting({required this.charges, required this.total});

  final List<DepreciationCharge> charges;

  /// The whole of it, so a Book that is behind says by how much.
  final String total;

  bool get isEmpty => charges.isEmpty;

  bool get isNotEmpty => charges.isNotEmpty;

  /// The months it is behind by, oldest first, each once.
  List<String> get months {
    final seen = <String>{};
    final months = <String>[];
    for (final charge in charges) {
      if (charge.period.trim().isEmpty) continue;
      if (seen.add(charge.period)) months.add(charge.period);
    }
    months.sort();
    return months;
  }

  factory DepreciationWaiting.fromJson(Map<String, dynamic> json) =>
      DepreciationWaiting(
        charges: ((json['unposted'] as List?) ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(DepreciationCharge.fromJson)
            .toList(),
        total: (json['total'] as String?) ?? '',
      );
}

/// Talking to the engine's depreciation journal.
class DepreciationPostingService {
  DepreciationPostingService._();

  static final DepreciationPostingService instance =
      DepreciationPostingService._();

  /// A second instance for a test that needs a transport of its own.
  DepreciationPostingService.fresh();

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

  /// What the register has recorded and the books have not.
  Future<DepreciationWaiting> waiting() async {
    final answer = await _client.get(
      Uri.parse('$_base/accounting/depreciation/unposted'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(_refusalOf(answer));
    }
    return DepreciationWaiting.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// Put one month's wear into the books.
  ///
  /// [period] is the month as `2026-11`. The figures are the register's; nothing here works out a charge.
  Future<DepreciationPosted> postMonth(String period) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/depreciation'),
      headers: _headers,
      body: jsonEncode({'period': period.trim()}),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return DepreciationPosted.fromJson(
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
    return 'The engine refused the depreciation: ${answer.statusCode}.';
  }
}
