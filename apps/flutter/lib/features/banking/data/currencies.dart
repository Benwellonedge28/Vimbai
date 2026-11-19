// The currencies this Book keeps money in, and every currency in the world it could choose.
//
// A currency is not a name alone: it declares how many decimal places its money has, and that is what keeps an amount
// exact. The app never decides that itself. It reads the world's catalogue from the engine and asks the engine to add
// the one the owner picks, so the places are the engine's answer rather than a guess of two.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One currency the world uses: its code, its name, and how many decimal places its money has.
class IsoCurrency {
  const IsoCurrency({
    required this.code,
    required this.name,
    required this.decimalPlaces,
  });

  final String code;
  final String name;
  final int decimalPlaces;

  factory IsoCurrency.fromJson(Map<String, dynamic> json) => IsoCurrency(
    code: (json['code'] as String?) ?? '',
    name: (json['name'] as String?) ?? '',
    decimalPlaces: _places(json['decimal_places']),
  );

  /// "two decimal places", "no decimal places", "three decimal places".
  String get placesWord => switch (decimalPlaces) {
    0 => 'no decimal places',
    1 => 'one decimal place',
    2 => 'two decimal places',
    3 => 'three decimal places',
    4 => 'four decimal places',
    _ => '$decimalPlaces decimal places',
  };

  /// "US Dollar (USD), money in two decimal places."
  String get reading => '$name ($code), $placesWord.';
}

int _places(Object? value) {
  if (value is int) return value;
  if (value is num) return value.toInt();
  if (value is String) return int.tryParse(value) ?? 2;
  return 2;
}

/// The currencies this Book keeps, as the engine holds them.
class CurrencyBook {
  const CurrencyBook(this.held);

  final List<IsoCurrency> held;

  static const CurrencyBook none = CurrencyBook([]);

  bool get empty => held.isEmpty;

  /// True when the Book already keeps this currency, so the offer is not made twice.
  bool holds(String code) =>
      held.any((one) => one.code.toUpperCase() == code.toUpperCase());

  factory CurrencyBook.fromJson(Object? body) {
    final rows = switch (body) {
      List<dynamic> list => list,
      Map<String, dynamic> map when map['currencies'] is List =>
        map['currencies'] as List<dynamic>,
      _ => const <dynamic>[],
    };
    return CurrencyBook([
      for (final row in rows)
        if (row is Map<String, dynamic> && (row['is_active'] ?? true) != false)
          IsoCurrency.fromJson(row),
    ]);
  }
}

/// Reads the Book's currencies and the world's catalogue, and adds one when the owner picks it.
class CurrencyService {
  CurrencyService._();

  static final CurrencyService instance = CurrencyService._();

  /// A second instance for a test that needs a transport of its own.
  CurrencyService.fresh();

  static const String _base = AppConfig.currencyRoute;

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

  /// Every currency in the world, so the owner chooses from the list rather than typing a code.
  Future<List<IsoCurrency>> catalogue() async {
    final answer = await _client.get(
      Uri.parse('$_base/currencies/catalogue'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(_reason(answer.body, answer.statusCode));
    }
    final body = jsonDecode(answer.body);
    final rows = switch (body) {
      List<dynamic> list => list,
      Map<String, dynamic> map when map['currencies'] is List =>
        map['currencies'] as List<dynamic>,
      _ => const <dynamic>[],
    };
    return [
      for (final row in rows)
        if (row is Map<String, dynamic>) IsoCurrency.fromJson(row),
    ];
  }

  /// The currencies this Book keeps.
  Future<CurrencyBook> read() async {
    final answer = await _client.get(
      Uri.parse('$_base/currencies'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(_reason(answer.body, answer.statusCode));
    }
    return CurrencyBook.fromJson(jsonDecode(answer.body));
  }

  /// Add a currency by its code. The engine fills in the name and the places that money actually has, and says why
  /// when it will not.
  Future<void> add(String code, {String today = ''}) async {
    final answer = await _client.post(
      Uri.parse('$_base/currencies'),
      headers: _headers,
      body: jsonEncode({'code': code, if (today.isNotEmpty) 'today': today}),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_reason(answer.body, answer.statusCode));
    }
  }

  String _reason(String body, int status) {
    try {
      final decoded = jsonDecode(body);
      if (decoded is Map<String, dynamic>) {
        for (final key in ['detail', 'error', 'message']) {
          final said = decoded[key];
          if (said is String && said.trim().isNotEmpty) return said;
        }
      }
    } catch (_) {
      // the body was not the engine speaking; the status is all there is
    }
    return 'The currencies could not be read just now. ($status)';
  }
}
