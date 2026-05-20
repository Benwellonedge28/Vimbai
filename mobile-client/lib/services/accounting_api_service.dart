import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:finacc_mobile_client/local_db/user_local_data.dart';
import 'package:finacc_mobile_client/config.dart'; // For API URL
import 'package:finacc_mobile_client/models/accounting_models.dart'; // NEW: Import Accounting Models

class AccountingApiService {
  final String _baseUrl = AppConfig.apiUrl; // Base URL from config
  final String _accountingServiceUrl = '${AppConfig.apiUrl.replaceFirst(':8080', ':8000')}'; // Hardcoded for now, will use API Gateway path later

  Future<Map<String, String>> _getHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer $token',
    };
  }

  // --- Chart of Accounts (COA) API Calls ---
  Future<List<Account>> getChartOfAccounts() async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_accountingServiceUrl/accounts/'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> accountsJson = json.decode(response.body);
      return accountsJson.map((json) => Account.fromJson(json)).toList();
    } else {
      throw Exception('Failed to load chart of accounts: ${response.body}');
    }
  }

  // --- Journal Entry API Calls ---
  Future<JournalEntry> createJournalEntry(JournalEntry entry) async {
    final headers = await _getHeaders();
    final response = await http.post(
      Uri.parse('$_accountingServiceUrl/journal-entries/'),
      headers: headers,
      body: json.encode(entry.toJson()),
    );

    if (response.statusCode == 201) {
      return JournalEntry.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to create journal entry: ${response.body}');
    }
  }

  Future<List<JournalEntry>> getJournalEntries() async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_accountingServiceUrl/journal-entries/'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> entriesJson = json.decode(response.body);
      return entriesJson.map((json) => JournalEntry.fromJson(json)).toList();
    } else {
      throw Exception('Failed to load journal entries: ${response.body}');
    }
  }

  // --- Ledger & Trial Balance API Calls (NEW) ---
  Future<LedgerAccountBalance> getLedgerAccountBalance(String accountNumber) async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_accountingServiceUrl/ledger/$accountNumber'), headers: headers);

    if (response.statusCode == 200) {
      return LedgerAccountBalance.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load ledger balance for $accountNumber: ${response.body}');
    }
  }

  Future<TrialBalance> getTrialBalance() async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_accountingServiceUrl/trial-balance/'), headers: headers);

    if (response.statusCode == 200) {
      return TrialBalance.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load trial balance: ${response.body}');
    }
  }
}
