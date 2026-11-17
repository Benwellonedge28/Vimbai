// Personal finance client: recurring transactions, debts, investments,
// tax estimation. The API gateway injects X-User-ID from the JWT.
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'dart:convert';
import 'package:vimbai_mobile_client/core/api/amount.dart';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

class PersonalFinanceService {
  PersonalFinanceService._();
  static final PersonalFinanceService instance = PersonalFinanceService._();

  // Route through the API gateway so the JWT is resolved to X-User-ID.
  static const String _kBaseUrl = String.fromEnvironment(
    'VIMBAI_PERSONAL_FINANCE_URL',
    defaultValue: '${AppConfig.apiUrl}/personal-finance',
  );

  String? _token;
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  http.Client _client = http.Client();

  /// A test calls the service against a transport it controls, so what the screen reads back from a reply
  /// is proven without a server. The real client is untouched until a test swaps it.
  @visibleForTesting
  void useClient(http.Client client) => _client = client;

  Uri _u(String p) => Uri.parse('$_kBaseUrl$p');

  Map<String, dynamic> _decode(http.Response r) {
    if (r.statusCode >= 400) {
      throw Exception('API ${r.statusCode}: ${r.body}');
    }
    return jsonDecode(r.body) as Map<String, dynamic>;
  }

  /// The engine answers these lists as a bare array, so reading `j['debts']` threw before the screen
  /// saw a row. Either shape is accepted, and a wrapped answer still reads under its own name.
  List<Map<String, dynamic>> _rows(http.Response r, String key) {
    if (r.statusCode >= 400) {
      throw Exception('API ${r.statusCode}: ${r.body}');
    }
    final decoded = jsonDecode(r.body);
    final list = decoded is List
        ? decoded
        : (decoded as Map<String, dynamic>)[key];
    return (list as List).cast<Map<String, dynamic>>();
  }

  // -- Recurring -----------------------------------------------------------

  Future<List<Map<String, dynamic>>> listRecurring() async {
    return _rows(
      await _client.get(_u('/recurring'), headers: _headers),
      'recurring',
    );
  }

  Future<void> createRecurring({
    required String kind,
    required String description,
    required ExactAmount amount,
    required String frequency,
    required String nextDue,
  }) async {
    await _client.post(
      _u('/recurring'),
      headers: _headers,
      body: jsonEncode({
        'kind': kind,
        'description': description,
        'amount': amount.toLedgerString(),
        'frequency': frequency,
        'next_due': nextDue,
      }),
    );
  }

  Future<Map<String, dynamic>> runRecurring(String id) async =>
      _decode(await _client.post(_u('/recurring/$id/run'), headers: _headers));

  Future<void> deleteRecurring(String id) async =>
      _client.delete(_u('/recurring/$id'), headers: _headers);

  // -- Debts ---------------------------------------------------------------

  Future<List<Map<String, dynamic>>> listDebts() async {
    return _rows(await _client.get(_u('/debts'), headers: _headers), 'debts');
  }

  Future<void> createDebt({
    required String name,
    required ExactAmount principal,
    required double annualRate,
    required int termMonths,
    required String startedAt,
  }) async {
    await _client.post(
      _u('/debts'),
      headers: _headers,
      body: jsonEncode({
        'name': name,
        'principal': principal.toLedgerString(),
        'annual_rate': annualRate,
        'term_months': termMonths,
        'started_at': startedAt,
      }),
    );
  }

  Future<Map<String, dynamic>> payDebt(String id, ExactAmount amount) async =>
      _decode(
        await _client.post(
          _u('/debts/$id/payments'),
          headers: _headers,
          body: jsonEncode({'amount': amount.toLedgerString()}),
        ),
      );

  // -- Investments ---------------------------------------------------------

  Future<Map<String, dynamic>> portfolio() async {
    final r = await _client.get(_u('/investments'), headers: _headers);
    if (r.statusCode >= 400) {
      throw Exception('API ${r.statusCode}: ${r.body}');
    }
    final decoded = jsonDecode(r.body);
    // the engine answers the holdings as a bare array; the page reads them under `investments`
    if (decoded is List) return {'investments': decoded};
    return decoded as Map<String, dynamic>;
  }

  Future<void> createInvestment({
    required String name,
    String assetClass = 'equity',
    double initialUnits = 0,
    ExactAmount initialPrice = ExactAmount.zero,
  }) async {
    await _client.post(
      _u('/investments'),
      headers: _headers,
      body: jsonEncode({
        'name': name,
        'asset_class': assetClass,
        'initial_units': initialUnits,
        'initial_price': initialPrice.toLedgerString(),
      }),
    );
  }

  Future<void> trade(
    String id, {
    required String side,
    required double units,
    required ExactAmount price,
  }) async {
    await _client.post(
      _u('/investments/$id/trades'),
      headers: _headers,
      body: jsonEncode({
        'side': side,
        'units': units,
        'price': price.toLedgerString(),
      }),
    );
  }

  Future<void> updatePrice(String id, ExactAmount price) async => _client.post(
    _u('/investments/$id/price'),
    headers: _headers,
    body: jsonEncode({'price': price.toLedgerString()}),
  );

  // -- Tax -----------------------------------------------------------------

  Future<List<Map<String, dynamic>>> brackets() async {
    final j = _decode(
      await _client.get(_u('/tax/brackets'), headers: _headers),
    );
    return (j['brackets'] as List).cast<Map<String, dynamic>>();
  }

  Future<Map<String, dynamic>> estimateTax({
    required ExactAmount annualIncome,
    ExactAmount deductions = ExactAmount.zero,
    ExactAmount payePaid = ExactAmount.zero,
  }) async => _decode(
    await _client.post(
      _u('/tax/estimate'),
      headers: _headers,
      body: jsonEncode({
        'annual_income': annualIncome.toLedgerString(),
        'deductions': deductions.toLedgerString(),
        'paye_paid': payePaid.toLedgerString(),
      }),
    ),
  );
}
