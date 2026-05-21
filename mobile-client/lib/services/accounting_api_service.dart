import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:finacc_mobile_client/local_db/user_local_data.dart';
import 'package:finacc_mobile_client/config.dart'; // For API URL
import 'package:finacc_mobile_client/models/accounting_models.dart'; // Import Accounting Models
import 'package:finacc_mobile_client/local_db/database_helper.dart'; // NEW
import 'package:connectivity_plus/connectivity_plus.dart'; // NEW
import 'package:uuid/uuid.dart'; // NEW

class AccountingApiService {
  final String _baseUrl = AppConfig.apiUrl; // Base URL from config
  final String _accountingServiceUrl = '${AppConfig.apiUrl}/accounts'; // Use API Gateway path prefix for accounts
  final String _journalEntriesServiceUrl = '${AppConfig.apiUrl}/journal-entries'; // Path prefix for journal entries
  final String _ledgerServiceUrl = '${AppConfig.apiUrl}/ledger'; // Path prefix for ledger
  final String _financialStatementsServiceUrl = '${AppConfig.apiUrl}/financial-statements'; // Path prefix for financial statements

  final DatabaseHelper _dbHelper = DatabaseHelper(); // NEW

  Future<Map<String, String>> _getHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer $token',
    };
  }

  // --- Chart of Accounts (COA) API Calls ---
  Future<List<Account>> getChartOfAccounts({bool forceRemote = false}) async { // NEW: forceRemote param
    final connectivityResult = await (Connectivity().checkConnectivity());
    if (!forceRemote && connectivityResult == ConnectivityResult.none) {
      // Return from local DB if offline
      print('Offline: Fetching accounts from local DB.');
      return await _dbHelper.getAccounts();
    }

    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_accountingServiceUrl/'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> accountsJson = json.decode(response.body);
      List<Account> accounts = accountsJson.map((json) => Account.fromJson(json)).toList();
      // Store in local DB
      print('Online: Fetched accounts from remote, updating local DB.');
      await _dbHelper.deleteAllAccounts(); // Clear old accounts
      for (var account in accounts) {
        await _dbHelper.insertAccount(account);
      }
      return accounts;
    } else {
      print('Failed to load chart of accounts: ${response.statusCode} - ${response.body}');
      throw Exception('Failed to load chart of accounts: ${response.body}');
    }
  }

  // --- Journal Entry API Calls ---
  Future<JournalEntry> createJournalEntry(JournalEntry entry, {bool isOffline = false}) async { // NEW: isOffline param
    if (isOffline) {
      // Generate a local ID if not present, and save to local DB
      final localEntry = entry.copyWith(id: entry.id ?? const Uuid().v4());
      await _dbHelper.insertJournalEntry(localEntry, isSynced: false);
      print('Offline: Stored journal entry locally: ${localEntry.description}');
      return localEntry;
    }

    // Try to send to remote
    try {
      final headers = await _getHeaders();
      final response = await http.post(
        Uri.parse('$_journalEntriesServiceUrl/'),
        headers: headers,
        body: json.encode(entry.toJson()),
      );

      if (response.statusCode == 201) {
        print('Online: Successfully created journal entry remotely: ${entry.description}');
        return JournalEntry.fromJson(json.decode(response.body));
      } else {
        print('Online: Failed to create journal entry remotely: ${response.statusCode} - ${response.body}');
        throw Exception('Failed to create journal entry: ${response.body}');
      }
    } catch (e) {
      print('Online: Error creating journal entry remotely, attempting local save: $e');
      // If remote creation fails, save it locally for later sync
      final localEntry = entry.copyWith(id: entry.id ?? const Uuid().v4()); // Ensure local ID
      await _dbHelper.insertJournalEntry(localEntry, isSynced: false);
      throw Exception('Failed to create journal entry remotely, saved offline: $e');
    }
  }

  Future<List<JournalEntry>> getJournalEntries({bool forceRemote = false}) async { // NEW: forceRemote param
    // For simplicity, Journal Entries are always fetched from remote for display
    // Offline entries are stored for syncing, not for direct display in main list
    // A more complex offline app would merge local and remote lists
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_journalEntriesServiceUrl/'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> entriesJson = json.decode(response.body);
      return entriesJson.map((json) => JournalEntry.fromJson(json)).toList();
    } else {
      throw Exception('Failed to load journal entries: ${response.body}');
    }
  }

  // --- Sync Offline Data (NEW) ---
  Future<void> syncOfflineJournalEntries() async {
    print('Attempting to sync offline journal entries...');
    final connectivityResult = await (Connectivity().checkConnectivity());
    if (connectivityResult == ConnectivityResult.none) {
      print('Sync failed: No internet connection.');
      throw Exception('No internet connection to sync data.');
    }

    final unsyncedEntries = await _dbHelper.getUnsyncedJournalEntries();
    if (unsyncedEntries.isEmpty) {
      print('No unsynced journal entries found.');
      return;
    }

    print('Found ${unsyncedEntries.length} unsynced entries. Starting sync...');
    for (var entry in unsyncedEntries) {
      try {
        // When syncing, we explicitly create the entry on the remote. 
        // The remote service will return the canonical ID. 
        // We don't use the local entry.id for remote creation to avoid conflicts 
        // if an ID was previously generated locally but not synced. 
        // We're essentially re-creating the entry on the server.
        await createJournalEntry(entry); 
        await _dbHelper.markJournalEntryAsSynced(entry.id!); // Mark as synced
        print('Successfully synced offline journal entry: ${entry.description}');
      } on http.ClientException catch (e) {
        print('Network error during sync for entry ${entry.id}: $e');
        // Could implement retry logic here
      } catch (e) {
        print('Failed to sync journal entry ${entry.id}: $e');
        // More granular error handling, e.g., if duplicate on server, mark as synced
        // For now, just log and continue. Consider user notification.
      }
    }
    print('Offline journal entry sync complete.');
  }

  // ... (Existing Ledger, Trial Balance, Financial Statement API Calls) ...
  Future<LedgerReport> getLedgerReport(String accountNumber, {DateTime? startDate, DateTime? endDate}) async {
    final headers = await _getHeaders();
    Map<String, String> queryParams = {};
    if (startDate != null) queryParams['start_date'] = startDate.toIso8601String();
    if (endDate != null) queryParams['end_date'] = endDate.toIso8601String();

    final uri = Uri.parse('$_ledgerServiceUrl/$accountNumber').replace(queryParameters: queryParams);
    final response = await http.get(uri, headers: headers);

    if (response.statusCode == 200) {
      return LedgerReport.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load ledger report: ${response.body}');
    }
  }

  Future<TrialBalanceReport> getTrialBalanceReport({DateTime? asOfDate}) async {
    final headers = await _getHeaders();
    Map<String, String> queryParams = {};
    if (asOfDate != null) queryParams['as_of_date'] = asOfDate.toIso8601String();

    final uri = Uri.parse('$_financialStatementsServiceUrl/trial-balance').replace(queryParameters: queryParams);
    final response = await http.get(uri, headers: headers);

    if (response.statusCode == 200) {
      return TrialBalanceReport.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load trial balance report: ${response.body}');
    }
  }

  Future<IncomeStatement> getIncomeStatement(DateTime startDate, DateTime endDate) async {
    final headers = await _getHeaders();
    final uri = Uri.parse('$_financialStatementsServiceUrl/income-statement')
        .replace(queryParameters: {
          'start_date': startDate.toIso8601String(),
          'end_date': endDate.toIso8601String(),
        });
    final response = await http.get(uri, headers: headers);

    if (response.statusCode == 200) {
      return IncomeStatement.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load income statement: ${response.body}');
    }
  }

  Future<BalanceSheet> getBalanceSheet(DateTime asOfDate) async {
    final headers = await _getHeaders();
    final uri = Uri.parse('$_financialStatementsServiceUrl/balance-sheet')
        .replace(queryParameters: {
          'as_of_date': asOfDate.toIso8601String(),
        });
    final response = await http.get(uri, headers: headers);

    if (response.statusCode == 200) {
      return BalanceSheet.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load balance sheet: ${response.body}');
    }
  }

}
