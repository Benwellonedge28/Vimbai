// Household finance client: period analysis of household incomes,
// expenses, assets and liabilities. The API gateway injects X-User-ID
// from the JWT and X-Book-ID from the active Book context.
import 'dart:convert';

import 'package:flutter/foundation.dart' show visibleForTesting;
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

class HouseholdFinanceService {
  HouseholdFinanceService._();

  /// A constructor a test can stand on: the analysis is a calculation the page hands the engine, and proving what the
  /// page sends should not need a running gateway.
  @visibleForTesting
  HouseholdFinanceService.forTest();

  static final HouseholdFinanceService instance = HouseholdFinanceService._();

  static const String _kBaseUrl = String.fromEnvironment(
    'VIMBAI_HOUSEHOLD_FINANCE_URL',
    // the household analysis is a calculation the engine answers, and `/calc` is the routed door to it
    defaultValue: '${AppConfig.apiUrl}/calc',
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
      _u('/household-finance-analyze'),
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
