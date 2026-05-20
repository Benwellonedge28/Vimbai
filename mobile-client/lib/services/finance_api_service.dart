import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:finacc_mobile_client/local_db/user_local_data.dart';
import 'package:finacc_mobile_client/config.dart'; // For API URL
import 'package:finacc_mobile_client/models/finance_models.dart'; // NEW: Import Finance Models

class FinanceApiService {
  final String _financeServiceUrl = '${AppConfig.apiUrl.replaceFirst(':8080', ':8001')}'; // Hardcoded for now

  Future<Map<String, String>> _getHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Authorization': 'Bearer $token',
    };
  }

  // --- Budget API Calls ---
  Future<List<Budget>> getBudgets() async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_financeServiceUrl/budgets/'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> budgetsJson = json.decode(response.body);
      return budgetsJson.map((json) => Budget.fromJson(json)).toList();
    } else {
      throw Exception('Failed to load budgets: ${response.body}');
    }
  }

  Future<Budget> getBudgetById(String budgetId) async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_financeServiceUrl/budgets/$budgetId'), headers: headers);

    if (response.statusCode == 200) {
      return Budget.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load budget $budgetId: ${response.body}');
    }
  }

  Future<Budget> createBudget(Budget budget) async {
    final headers = await _getHeaders();
    final response = await http.post(
      Uri.parse('$_financeServiceUrl/budgets/'),
      headers: headers,
      body: json.encode(budget.toJson()),
    );

    if (response.statusCode == 201) {
      return Budget.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to create budget: ${response.body}');
    }
  }

  // --- Variance Analysis API Calls (NEW ADDITION) ---
  Future<BudgetVarianceReport> getBudgetVarianceReport(String budgetId) async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_financeServiceUrl/budgets/$budgetId/variance-report'), headers: headers);

    if (response.statusCode == 200) {
      return BudgetVarianceReport.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load budget variance report: ${response.body}');
    }
  }
}
