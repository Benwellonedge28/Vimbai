import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:finacc_mobile_client/local_db/user_local_data.dart';
import 'package:finacc_mobile_client/config.dart'; // For API URL
import 'package:finacc_mobile_client/models/finance_models.dart'; // Import Finance Models
import 'package:connectivity_plus/connectivity_plus.dart'; // For offline detection - NEW
import 'package:finacc_mobile_client/local_db/database_helper.dart'; // For local DB access - NEW

class FinanceApiService {
  final String _financeServiceUrl = '${AppConfig.apiUrl}/budgets'; // Use API Gateway path prefix for budgets
  final String _financialRatiosUrl = '${AppConfig.apiUrl}/financial-ratios'; // Use API Gateway path prefix for financial ratios

  final DatabaseHelper _dbHelper = DatabaseHelper(); // NEW: For local budget storage

  Future<Map<String, String>> _getHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Content-Type': 'application/json', // Ensure content type is set
      'Authorization': 'Bearer $token',
    };
  }

  // --- Budget API Calls (NEW ADDITIONS) ---
  Future<Budget> createBudget(BudgetCreate budget) async {
    final headers = await _getHeaders();
    final response = await http.post(
      Uri.parse('$_financeServiceUrl/'),
      headers: headers,
      body: json.encode(budget.toJson()),
    );

    if (response.statusCode == 201) {
      return Budget.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to create budget: ${response.body}');
    }
  }

  Future<List<Budget>> getBudgets() async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_financeServiceUrl/'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> budgetsJson = json.decode(response.body);
      return budgetsJson.map((json) => Budget.fromJson(json)).toList();
    } else {
      throw Exception('Failed to load budgets: ${response.body}');
    }
  }

  Future<Budget> getBudgetById(String budgetId) async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_financeServiceUrl/$budgetId'), headers: headers);

    if (response.statusCode == 200) {
      return Budget.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load budget: ${response.body}');
    }
  }

  Future<Budget> updateBudget(String budgetId, BudgetUpdate budget) async {
    final headers = await _getHeaders();
    final response = await http.put(
      Uri.parse('$_financeServiceUrl/$budgetId'),
      headers: headers,
      body: json.encode(budget.toJson()),
    );

    if (response.statusCode == 200) {
      return Budget.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to update budget: ${response.body}');
    }
  }

  Future<void> deleteBudget(String budgetId) async {
    final headers = await _getHeaders();
    final response = await http.delete(
      Uri.parse('$_financeServiceUrl/$budgetId'),
      headers: headers,
    );

    if (response.statusCode != 204) {
      throw Exception('Failed to delete budget: ${response.body}');
    }
  }

  // --- Budget Item API Calls (NEW ADDITIONS) ---
  Future<BudgetItem> createBudgetItem(String budgetId, BudgetItemCreate item) async {
    final headers = await _getHeaders();
    final response = await http.post(
      Uri.parse('$_financeServiceUrl/$budgetId/items/'),
      headers: headers,
      body: json.encode(item.toJson()),
    );

    if (response.statusCode == 201) {
      return BudgetItem.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to create budget item: ${response.body}');
    }
  }

  Future<BudgetItem> updateBudgetItem(String budgetId, String itemId, BudgetItemUpdate item) async {
    final headers = await _getHeaders();
    final response = await http.put(
      Uri.parse('$_financeServiceUrl/$budgetId/items/$itemId'),
      headers: headers,
      body: json.encode(item.toJson()),
    );

    if (response.statusCode == 200) {
      return BudgetItem.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to update budget item: ${response.body}');
    }
  }

  Future<void> deleteBudgetItem(String budgetId, String itemId) async {
    final headers = await _getHeaders();
    final response = await http.delete(
      Uri.parse('$_financeServiceUrl/$budgetId/items/$itemId'),
      headers: headers,
    );

    if (response.statusCode != 204) {
      throw Exception('Failed to delete budget item: ${response.body}');
    }
  }

  // --- Variance Analysis API Calls ---
  Future<BudgetVarianceReport> getBudgetVarianceReport(String budgetId) async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_financeServiceUrl/$budgetId/variance-report'), headers: headers);

    if (response.statusCode == 200) {
      return BudgetVarianceReport.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load budget variance report: ${response.body}');
    }
  }

  // --- Financial Ratios API Calls ---
  Future<FinancialRatiosReport> getFinancialRatios(DateTime startDate, DateTime endDate) async {
    final headers = await _getHeaders();
    final response = await http.get(
      Uri.parse('$_financialRatiosUrl?start_date=${startDate.toIso8601String()}&end_date=${endDate.toIso8601String()}'),
      headers: headers,
    );

    if (response.statusCode == 200) {
      return FinancialRatiosReport.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load financial ratios report: ${response.body}');
    }
  }
}
