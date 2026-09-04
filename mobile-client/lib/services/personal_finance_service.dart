// Personal finance client: recurring transactions, debts, investments,
// tax estimation. The API gateway injects X-User-ID from the JWT.
import 'dart:convert';

import 'package:http/http.dart' as http;

class PersonalFinanceService {
  PersonalFinanceService._();
  static final PersonalFinanceService instance = PersonalFinanceService._();

  static const String _kBaseUrl = String.fromEnvironment(
    'VIMBAI_PERSONAL_FINANCE_URL',
    defaultValue: 'http://10.0.2.2:8080/personal-finance',
  );

  String? _token;
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
        'Content-Type': 'application/json',
        if (_token != null) 'Authorization': 'Bearer $_token',
      };

  http.Client _client = http.Client();

  Uri _u(String p) => Uri.parse('$_kBaseUrl$p');

  Map<String, dynamic> _decode(http.Response r) {
    if (r.statusCode >= 400) {
      throw Exception('API ${r.statusCode}: ${r.body}');
    }
    return jsonDecode(r.body) as Map<String, dynamic>;
  }

  // -- Recurring -----------------------------------------------------------

  Future<List<Map<String, dynamic>>> listRecurring() async {
    final j = _decode(await _client.get(_u('/recurring'), headers: _headers));
    return (j['recurring'] as List).cast<Map<String, dynamic>>();
  }

  Future<void> createRecurring({
    required String kind,
    required String description,
    required double amount,
    required String frequency,
    required String nextDue,
  }) async {
    await _client.post(
      _u('/recurring'),
      headers: _headers,
      body: jsonEncode({
        'kind': kind,
        'description': description,
        'amount': amount,
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
    final j = _decode(await _client.get(_u('/debts'), headers: _headers));
    return (j['debts'] as List).cast<Map<String, dynamic>>();
  }

  Future<void> createDebt({
    required String name,
    required double principal,
    required double annualRate,
    required int termMonths,
    required String startedAt,
  }) async {
    await _client.post(
      _u('/debts'),
      headers: _headers,
      body: jsonEncode({
        'name': name,
        'principal': principal,
        'annual_rate': annualRate,
        'term_months': termMonths,
        'started_at': startedAt,
      }),
    );
  }

  Future<Map<String, dynamic>> payDebt(String id, double amount) async =>
      _decode(await _client.post(
        _u('/debts/$id/payments'),
        headers: _headers,
        body: jsonEncode({'amount': amount}),
      ));

  // -- Investments ---------------------------------------------------------

  Future<Map<String, dynamic>> portfolio() async =>
      _decode(await _client.get(_u('/investments'), headers: _headers));

  Future<void> createInvestment({
    required String name,
    String assetClass = 'equity',
    double initialUnits = 0,
    double initialPrice = 0,
  }) async {
    await _client.post(
      _u('/investments'),
      headers: _headers,
      body: jsonEncode({
        'name': name,
        'asset_class': assetClass,
        'initial_units': initialUnits,
        'initial_price': initialPrice,
      }),
    );
  }

  Future<void> trade(
    String id, {
    required String side,
    required double units,
    required double price,
  }) async {
    await _client.post(
      _u('/investments/$id/trades'),
      headers: _headers,
      body: jsonEncode({'side': side, 'units': units, 'price': price}),
    );
  }

  Future<void> updatePrice(String id, double price) async =>
      _client.post(
        _u('/investments/$id/price'),
        headers: _headers,
        body: jsonEncode({'price': price}),
      );

  // -- Tax -----------------------------------------------------------------

  Future<List<Map<String, dynamic>>> brackets() async {
    final j = _decode(await _client.get(_u('/tax/brackets'), headers: _headers));
    return (j['brackets'] as List).cast<Map<String, dynamic>>();
  }

  Future<Map<String, dynamic>> estimateTax({
    required double annualIncome,
    double deductions = 0,
    double payePaid = 0,
  }) async =>
      _decode(await _client.post(
        _u('/tax/estimate'),
        headers: _headers,
        body: jsonEncode({
          'annual_income': annualIncome,
          'deductions': deductions,
          'paye_paid': payePaid,
        }),
      ));
}
