// Household finance client: period analysis of household incomes,
// expenses, assets and liabilities. The API gateway injects X-User-ID
// from the JWT and X-Book-ID from the active Book context.
import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/config.dart';
import 'package:vimbai_mobile_client/services/book_context.dart';

class HouseholdFinanceService {
  HouseholdFinanceService._();
  static final HouseholdFinanceService instance = HouseholdFinanceService._();

  static const String _kBaseUrl = String.fromEnvironment(
    'VIMBAI_HOUSEHOLD_FINANCE_URL',
    defaultValue: '${AppConfig.apiUrl}/household-finance',
  );

  String? _token;
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
        'Content-Type': 'application/json',
        if (_token != null) 'Authorization': 'Bearer $_token',
        ...BookContext.instance.headers(),
      };

  final http.Client _client = http.Client();

  Uri _u(String p) => Uri.parse('$_kBaseUrl$p');

  /// Analyze a household's finances for a period.
  ///
  /// [incomes] / [expenses] / [assets] / [liabilities] are lists of maps
  /// matching the backend models (HouseholdIncome, HouseholdExpense,
  /// Asset, Liability).
  Future<Map<String, dynamic>> analyze({
    required String householdId,
    required String period,
    List<Map<String, dynamic>> incomes = const [],
    List<Map<String, dynamic>> expenses = const [],
    List<Map<String, dynamic>> assets = const [],
    List<Map<String, dynamic>> liabilities = const [],
  }) async {
    final r = await _client.post(
      _u('/analyze'),
      headers: _headers,
      body: jsonEncode({
        'household_id': householdId,
        'period': period,
        'incomes': incomes,
        'expenses': expenses,
        'assets': assets,
        'liabilities': liabilities,
      }),
    );
    if (r.statusCode >= 400) {
      throw Exception('API ${r.statusCode}: ${r.body}');
    }
    return jsonDecode(r.body) as Map<String, dynamic>;
  }
}
