import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:finacc_mobile_client/local_db/user_local_data.dart';
import 'package:finacc_mobile_client/config.dart'; // For API URL
import 'package:finacc_mobile_client/models/finance_models.dart'; // Import Finance Models

class FinanceApiService {
  final String _financeServiceUrl = '${AppConfig.apiUrl}/budgets'; // Use API Gateway path prefix for budgets
  final String _financialRatiosUrl = '${AppConfig.apiUrl}/financial-ratios'; // Use API Gateway path prefix for financial ratios

  Future<Map<String, String>> _getHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Authorization': 'Bearer $token',
    };
  }

  // ... (existing Budget API Calls) ...

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

  // --- Financial Ratios API Calls (NEW ADDITION) ---
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
