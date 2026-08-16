import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/config.dart'; // For API URL
import 'package:vimbai_mobile_client/models/banking_models.dart'; // Import Banking Models

class BankingApiService {
  final String _bankingServiceUrl = '${AppConfig.apiUrl}/banking/accounts'; // Via API Gateway

  Future<Map<String, String>> _getHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer $token',
    };
  }

  // --- Bank Account API Calls ---
  Future<BankAccount> createBankAccount(BankAccount newAccount) async {
    final headers = await _getHeaders();
    final response = await http.post(
      Uri.parse('$_bankingServiceUrl/'),
      headers: headers,
      body: json.encode(newAccount.toJson()),
    );

    if (response.statusCode == 201) {
      return BankAccount.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to create bank account: ${response.body}');
    }
  }

  Future<List<BankAccount>> getBankAccounts() async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_bankingServiceUrl/'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> accountsJson = json.decode(response.body);
      return accountsJson.map((json) => BankAccount.fromJson(json)).toList();
    } else {
      throw Exception('Failed to load bank accounts: ${response.body}');
    }
  }

  // --- Bank Transaction API Calls ---
  Future<List<BankTransaction>> fetchAndStoreTransactions(String accountId) async {
    final headers = await _getHeaders();
    final response = await http.post(
      Uri.parse('$_bankingServiceUrl/$accountId/fetch-transactions'),
      headers: headers,
    );

    if (response.statusCode == 200) {
      List<dynamic> transactionsJson = json.decode(response.body);
      return transactionsJson.map((json) => BankTransaction.fromJson(json)).toList();
    } else {
      throw Exception('Failed to fetch and store transactions: ${response.body}');
    }
  }

  Future<List<BankTransaction>> getTransactionsForAccount(String accountId) async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_bankingServiceUrl/$accountId/transactions'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> transactionsJson = json.decode(response.body);
      return transactionsJson.map((json) => BankTransaction.fromJson(json)).toList();
    } else {
      throw Exception('Failed to load transactions for account $accountId: ${response.body}');
    }
  }
}
