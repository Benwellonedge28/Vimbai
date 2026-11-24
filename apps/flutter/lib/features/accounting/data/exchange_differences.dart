// What the rate did to the books, read from the engine.
//
// A business that trades in another money makes and loses money on the rate itself, and the engine already knows which part
// of that is money in hand and which is only a figure on paper: a difference on something settled has been paid, and a
// difference on a balance still held could move again before it ever is. This reading adds no arithmetic. It keeps those
// two apart, because a person shown one total will treat an unrealised gain as though it were cash.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// A gain and a loss, kept apart rather than netted.
class GainLoss {
  const GainLoss({required this.gain, required this.loss, required this.net});

  final ExactAmount gain;
  final ExactAmount loss;
  final ExactAmount net;

  bool get isQuiet => gain.isZero && loss.isZero;

  factory GainLoss.fromJson(Map<String, dynamic> json) => GainLoss(
    gain: ExactAmount.parse((json['gain'] as String?) ?? '0'),
    loss: ExactAmount.parse((json['loss'] as String?) ?? '0'),
    net: ExactAmount.parse((json['net'] as String?) ?? '0'),
  );
}

/// One entry the rate moved.
class RateEntry {
  const RateEntry({
    required this.date,
    required this.reference,
    required this.description,
    required this.settled,
    required this.gain,
    required this.loss,
  });

  final String date;
  final String reference;
  final String description;

  /// Whether the difference was on something already paid, rather than on a balance still held.
  final bool settled;

  final ExactAmount gain;
  final ExactAmount loss;

  factory RateEntry.fromJson(Map<String, dynamic> json) => RateEntry(
    date: (json['entry_date'] as String?) ?? '',
    reference: (json['reference_number'] as String?) ?? '',
    description: (json['description'] as String?) ?? '',
    settled: (json['settled'] as bool?) ?? false,
    gain: ExactAmount.parse((json['gain'] as String?) ?? '0'),
    loss: ExactAmount.parse((json['loss'] as String?) ?? '0'),
  );
}

/// What the rate did over a period.
class ExchangeDifferences {
  const ExchangeDifferences({
    required this.from,
    required this.to,
    required this.everything,
    required this.settled,
    required this.held,
    required this.entries,
  });

  final String from;
  final String to;

  /// The whole period, both kinds together.
  final GainLoss everything;

  /// Differences on documents that have been paid: this is money.
  final GainLoss settled;

  /// Differences on balances still held: this is a figure that could move again.
  final GainLoss held;

  final List<RateEntry> entries;

  bool get isQuiet => entries.isEmpty;

  factory ExchangeDifferences.fromJson(Map<String, dynamic> json) {
    final entries = (json['entries'] as List<dynamic>? ?? const [])
        .whereType<Map<String, dynamic>>()
        .map(RateEntry.fromJson)
        .toList();
    return ExchangeDifferences(
      from: (json['from'] as String?) ?? '',
      to: (json['to'] as String?) ?? '',
      everything: GainLoss.fromJson(
        (json['gain'] == null && json['loss'] == null)
            ? const {}
            : {'gain': json['gain'], 'loss': json['loss'], 'net': json['net']},
      ),
      settled: GainLoss.fromJson(
        (json['settled'] as Map<String, dynamic>?) ?? const {},
      ),
      held: GainLoss.fromJson(
        (json['held'] as Map<String, dynamic>?) ?? const {},
      ),
      entries: entries,
    );
  }
}

/// Reading what the rate did, from the engine's own report.
class ExchangeDifferencesService {
  ExchangeDifferencesService._();

  static final ExchangeDifferencesService instance =
      ExchangeDifferencesService._();

  /// A second instance for a test that needs a transport of its own.
  ExchangeDifferencesService.fresh();

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

  /// What the rate did between two days, both counted.
  Future<ExchangeDifferences> forPeriod(DateTime from, DateTime to) async {
    final answer = await _client.get(
      Uri.parse(
        '$_base/accounting/exchange-differences'
        '?from=${_day(from)}&to=${_day(to)}',
      ),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(_refusalOf(answer));
    }
    return ExchangeDifferences.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  static String _day(DateTime date) =>
      '${date.year.toString().padLeft(4, '0')}-'
      '${date.month.toString().padLeft(2, '0')}-'
      '${date.day.toString().padLeft(2, '0')}';

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
    return 'The engine refused the reading: ${answer.statusCode}.';
  }
}
