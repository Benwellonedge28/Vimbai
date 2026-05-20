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
      return await _dbHelper.getAccounts();
    }

    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_accountingServiceUrl/'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> accountsJson = json.decode(response.body);
      List<Account> accounts = accountsJson.map((json) => Account.fromJson(json)).toList();
      // Store in local DB
      await _dbHelper.deleteAllAccounts(); // Clear old accounts
      for (var account in accounts) {
        await _dbHelper.insertAccount(account);
      }
      return accounts;
    } else {
      throw Exception('Failed to load chart of accounts: ${response.body}');
    }
  }

  // --- Journal Entry API Calls ---
  Future<JournalEntry> createJournalEntry(JournalEntry entry, {bool isOffline = false}) async { // NEW: isOffline param
    if (isOffline) {
      await _dbHelper.insertJournalEntry(entry, isSynced: false);
      // Return a dummy entry or the passed entry with a local ID
      return JournalEntry(
        id: entry.id ?? const Uuid().v4(), // Generate local UUID if not provided
        entryDate: entry.entryDate,
        description: entry.description,
        referenceNumber: entry.referenceNumber,
        sourceModule: entry.sourceModule,
        lines: entry.lines,
        createdAt: DateTime.now(),
        updatedAt: DateTime.now(),
      );
    }

    final headers = await _getHeaders();
    final response = await http.post(
      Uri.parse('$_journalEntriesServiceUrl/'),
      headers: headers,
      body: json.encode(entry.toJson()),
    );

    if (response.statusCode == 201) {
      return JournalEntry.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to create journal entry: ${response.body}');
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
    final connectivityResult = await (Connectivity().checkConnectivity());
    if (connectivityResult == ConnectivityResult.none) {
      throw Exception("No internet connection to sync data.");
    }

    final unsyncedEntries = await _dbHelper.getUnsyncedJournalEntries();
    for (var entry in unsyncedEntries) {
      try {
        await createJournalEntry(entry); // Send to remote
        await _dbHelper.markJournalEntryAsSynced(entry.id!); // Mark as synced
        print("Synced offline journal entry: ${entry.description}");
      } catch (e) {
        print("Failed to sync journal entry ${entry.id}: $e");
        // Handle specific errors (e.g., duplicate, validation failed)
        // For now, just log and continue.
      }
    }
  }

  // ... (Existing Ledger, Trial Balance, Financial Statement API Calls) ...
}
