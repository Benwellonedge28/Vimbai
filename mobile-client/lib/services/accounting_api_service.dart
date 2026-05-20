import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:finacc_mobile_client/local_db/user_local_data.dart';
import 'package:finacc_mobile_client/config.dart'; // For API URL
import 'package:finacc_mobile_client/models/accounting_models.dart'; // NEW: Import Accounting Models

class AccountingApiService {
  final String _baseUrl = AppConfig.apiUrl; // Base URL from config
  // final String _accountingServiceUrl = '${AppConfig.apiUrl.replaceFirst(':8080', ':8000')}'; // Old hardcoded derivation
  final String _accountingServiceUrl = '${AppConfig.apiUrl}/accounts'; // NEW: Use API Gateway path prefix for accounts
  final String _journalEntriesServiceUrl = '${AppConfig.apiUrl}/journal-entries'; // NEW: Path prefix for journal entries
  final String _ledgerServiceUrl = '${AppConfig.apiUrl}/ledger'; // NEW: Path prefix for ledger
  final String _financialStatementsServiceUrl = '${AppConfig.apiUrl}/financial-statements'; // NEW: Path prefix for financial statements

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
    final response = await http.get(Uri.parse('$_accountingServiceUrl/'), headers: headers); // Note the trailing slash

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
      Uri.parse('$_journalEntriesServiceUrl/'), // Note the trailing slash
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
    final response = await http.get(Uri.parse('$_journalEntriesServiceUrl/'), headers: headers); // Note the trailing slash

    if (response.statusCode == 200) {
      List<dynamic> entriesJson = json.decode(response.body);
      return entriesJson.map((json) => JournalEntry.fromJson(json)).toList();
    } else {
      throw Exception('Failed to load journal entries: ${response.body}');
    }
  }

  // --- Ledger & Trial Balance API Calls ---
  Future<LedgerAccountBalance> getLedgerAccountBalance(String accountNumber) async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_ledgerServiceUrl/$accountNumber'), headers: headers);

    if (response.statusCode == 200) {
      return LedgerAccountBalance.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load ledger balance for $accountNumber: ${response.body}');
    }
  }

  Future<TrialBalance> getTrialBalance() async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('${AppConfig.apiUrl}/trial-balance/'), headers: headers); // Corrected to use gateway direct path for TB

    if (response.statusCode == 200) {
      return TrialBalance.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load trial balance: ${response.body}');
    }
  }

  // --- Financial Statement API Calls (NEW ADDITIONS) ---
  Future<IncomeStatement> getIncomeStatement(DateTime startDate, DateTime endDate) async {
    final headers = await _getHeaders();
    final response = await http.get(
      Uri.parse('$_financialStatementsServiceUrl/income-statement?start_date=${startDate.toIso8601String()}&end_date=${endDate.toIso8601String()}'),
      headers: headers,
    );

    if (response.statusCode == 200) {
      return IncomeStatement.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load income statement: ${response.body}');
    }
  }

  Future<BalanceSheet> getBalanceSheet(DateTime asOfDate) async {
    final headers = await _getHeaders();
    final response = await http.get(
      Uri.parse('$_financialStatementsServiceUrl/balance-sheet?as_of_date=${asOfDate.toIso8601String()}'),
      headers: headers,
    );

    if (response.statusCode == 200) {
      return BalanceSheet.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load balance sheet: ${response.body}');
    }
  }

  Future<CashFlowStatement> getCashFlowStatement(DateTime startDate, DateTime endDate) async {
    final headers = await _getHeaders();
    final response = await http.get(
      Uri.parse('$_financialStatementsServiceUrl/cash-flow-statement?start_date=${startDate.toIso8601String()}&end_date=${endDate.toIso8601String()}'),
      headers: headers,
    );

    if (response.statusCode == 200) {
      return CashFlowStatement.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load cash flow statement: ${response.body}');
    }
  }
}
