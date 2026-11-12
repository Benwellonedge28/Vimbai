// mobile-client/lib/services/accounting_api_service.dart
//
// Offline-first accounting service.
//
// Reads: network-first with SQLite caching. When the device is offline
// (ApiClient throws OfflineException), cached data from the last
// successful sync is returned instead of an error.
// Writes: posted to the server when online; when offline the entry is
// stored locally with is_synced=0 and picked up by SyncService later.

import 'dart:convert';
import 'package:vimbai_mobile_client/config.dart';
import 'package:vimbai_mobile_client/services/api_client.dart';
import 'package:vimbai_mobile_client/models/accounting_models.dart';
import 'package:vimbai_mobile_client/services/auth_service.dart';
import 'package:vimbai_mobile_client/local_db/database_helper.dart';
import 'package:vimbai_mobile_client/services/book_context.dart';

class AccountingApiService {
  final String _baseUrl = AppConfig.accountingRoute;
  final ApiClient _client = ApiClient();
  final AuthService _authService = AuthService();
  final DatabaseHelper _localDb = DatabaseHelper();

  Future<Map<String, String>> _getHeaders() async {
    final token = await _authService.getToken();
    return {
      'Content-Type': 'application/json',
      if (token != null) 'Authorization': 'Bearer $token',
      ...BookContext.instance.headers(),
    };
  }

  // ------------------------------------------------------------------
  // Journal Entries
  // ------------------------------------------------------------------

  /// Creates a journal entry. When offline (or [isOffline] is forced by the
  /// caller after a connectivity check), the entry is stored locally for
  /// later sync instead of being rejected.
  /// Offline-created entries get a client-generated reference number so a
  /// replayed push is idempotent: the server dedups on reference and the
  /// retry turns into a 409 the client treats as already-synced.
  JournalEntry _withClientReference(JournalEntry entry) {
    if ((entry.referenceNumber ?? '').isNotEmpty) return entry;
    return JournalEntry(
      id: entry.id,
      entryDate: entry.entryDate,
      description: entry.description,
      referenceNumber: 'MOB-${entry.id}',
      sourceModule: entry.sourceModule,
      lines: entry.lines,
    );
  }

  Future<JournalEntry> createJournalEntry(JournalEntry entry, {bool isOffline = false}) async {
    if (isOffline) {
      entry = _withClientReference(entry);
      await _localDb.insertJournalEntry(entry, isSynced: false);
      return entry;
    }
    try {
      final response = await _client.post(
        Uri.parse('$_baseUrl/journal-entries/'),
        headers: await _getHeaders(),
        body: json.encode(entry.toJson()),
      );
      if (response.statusCode == 201 || response.statusCode == 200) {
        final saved = JournalEntry.fromJson(json.decode(response.body) as Map<String, dynamic>);
        await _localDb.insertJournalEntry(saved, isSynced: true);
        return saved;
      }
      throw Exception('Failed to create journal entry: ${response.body}');
    } on OfflineException {
      // Network dropped mid-flight: keep the entry locally for later sync.
      entry = _withClientReference(entry);
      await _localDb.insertJournalEntry(entry, isSynced: false);
      return entry;
    }
  }

  /// Pushes a locally-created (unsynced) entry to the server. Used by
  /// SyncService. On success the local copy is marked as synced.
  Future<void> pushJournalEntryToServer(JournalEntry entry, {String? bookId}) async {
    final headers = await _getHeaders();
    if (bookId != null) headers['X-Book-ID'] = bookId;
    final response = await _client.post(
      Uri.parse('$_baseUrl/journal-entries/'),
      headers: headers,
      body: json.encode(entry.toJson()),
    );
    if (response.statusCode == 201 || response.statusCode == 200) {
      await _localDb.markJournalEntryAsSynced(entry.id);
    } else if (response.statusCode == 409) {
      // Idempotent replay: the server already stored this entry under the
      // same reference number (e.g. a previous push succeeded but the
      // response was lost). Treat as synced instead of duplicating.
      await _localDb.markJournalEntryAsSynced(entry.id);
    } else {
      throw Exception('Failed to sync journal entry: ${response.body}');
    }
  }

  /// Network-first list of journal entries with offline cache fallback.
  Future<List<JournalEntry>> getJournalEntries() async {
    try {
      final response = await _client.get(
        Uri.parse('$_baseUrl/journal-entries/'),
        headers: await _getHeaders(),
      );
      if (response.statusCode == 200) {
        final List<dynamic> body = json.decode(response.body);
        final entries = body
            .map((model) => JournalEntry.fromJson(model as Map<String, dynamic>))
            .toList();
        // Refresh the local cache for offline use.
        await _refreshLocalJournalCache(entries);
        return entries;
      }
      throw Exception('Failed to get journal entries: ${response.body}');
    } on OfflineException {
      return _localDb.getAllJournalEntriesFromLocal();
    }
  }

  /// Full server detail for one entry (no offline fallback: the form pages
  /// only need this when inspecting a specific record).
  Future<JournalEntry> getJournalEntry(String entryId) async {
    final response = await _client.get(
      Uri.parse('$_baseUrl/journal-entries/$entryId'),
      headers: await _getHeaders(),
    );
    if (response.statusCode == 200) {
      return JournalEntry.fromJson(json.decode(response.body) as Map<String, dynamic>);
    }
    throw Exception('Failed to get journal entry: ${response.body}');
  }

  Future<JournalEntryInDB> updateJournalEntry(String entryId, JournalEntryUpdate entry) async {
    final response = await _client.put(
      Uri.parse('$_baseUrl/journal-entries/$entryId'),
      headers: await _getHeaders(),
      body: json.encode(entry.toJson()),
    );
    if (response.statusCode == 200) {
      return JournalEntryInDB.fromJson(json.decode(response.body) as Map<String, dynamic>);
    }
    throw Exception('Failed to update journal entry: ${response.body}');
  }

  Future<void> deleteJournalEntry(String entryId) async {
    final response = await _client.delete(
      Uri.parse('$_baseUrl/journal-entries/$entryId'),
      headers: await _getHeaders(),
    );
    if (response.statusCode != 204) {
      throw Exception('Failed to delete journal entry: ${response.body}');
    }
  }

  Future<void> _refreshLocalJournalCache(List<JournalEntry> entries) async {
    final db = await _localDb.database;
    // Replace cached (synced) rows; keep unsynced local-only rows.
    await db.delete('journal_entries', where: 'is_synced = ?', whereArgs: [1]);
    await db.delete('journal_lines');
    for (final entry in entries) {
      await _localDb.insertJournalEntry(entry, isSynced: true);
    }
  }

  // ------------------------------------------------------------------
  // Chart of Accounts
  // ------------------------------------------------------------------


  /// Fetches the latest cash flow statement from the cash flow statement
  /// service (via the API gateway). If none exists yet, an empty statement
  /// is generated server-side for the period.
  Future<CashFlowStatement> getCashFlowStatement(DateTime startDate, DateTime endDate) async {
    const companyId = 'default';
    final headers = await _getHeaders();
    // Try the latest stored statement first.
    try {
      final response = await _client.get(
        Uri.parse('${AppConfig.cashFlowStatementRoute}/latest/$companyId'),
        headers: headers,
      );
      if (response.statusCode == 200) {
        return _mapCashFlowResponse(json.decode(response.body));
      }
    } catch (_) {
      // fall through to generate a fresh one
    }

    // No stored statement yet: ask the service to generate one for the period.
    final response = await _client.post(
      Uri.parse('${AppConfig.cashFlowStatementRoute}/generate'),
      headers: headers,
      body: json.encode({
        'company_id': companyId,
        'period_start': startDate.toIso8601String(),
        'period_end': endDate.toIso8601String(),
      }),
    );
    if (response.statusCode != 200 && response.statusCode != 201) {
      throw Exception('Failed to get cash flow statement: ${response.body}');
    }
    return _mapCashFlowResponse(json.decode(response.body));
  }

  CashFlowStatement _mapCashFlowResponse(Map<String, dynamic> json) {
    CashFlowSection toSection(String title, List<dynamic>? lines) {
      return CashFlowSection(
        title: title,
        activities: (lines ?? [])
            .map((l) => CashFlowActivity(
                  description: l['description'] ?? '',
                  amount: (l['amount'] as num?)?.toDouble() ?? 0.0,
                ))
            .toList(),
      );
    }

    return CashFlowStatement(
      startDate: DateTime.tryParse(json['period_start'] ?? '') ?? DateTime.now(),
      endDate: DateTime.tryParse(json['period_end'] ?? '') ?? DateTime.now(),
      reportDate: DateTime.tryParse(json['period_end'] ?? '') ?? DateTime.now(),
      operatingActivities: toSection('Operating', json['operating_activities']),
      investingActivities: toSection('Investing', json['investing_activities']),
      financingActivities: toSection('Financing', json['financing_activities']),
      netIncreaseDecreaseInCash:
          (json['net_change'] as num?)?.toDouble() ?? 0.0,
      beginningCashBalance:
          (json['beginning_cash'] as num?)?.toDouble() ?? 0.0,
      endingCashBalance: (json['ending_cash'] as num?)?.toDouble() ?? 0.0,
      netIncome: (json['net_operating'] as num?)?.toDouble() ?? 0.0,
    );
  }

  /// Pushes any locally-queued (unsynced) journal entries to the backend.
  /// Returns the number of entries successfully synced.
  Future<int> syncOfflineJournalEntries() async {
    final unsynced = await _localDb.getUnsyncedJournalEntries();
    var synced = 0;
    for (final entry in unsynced) {
      try {
        await pushJournalEntryToServer(entry);
        await _localDb.markJournalEntryAsSynced(entry.id);
        synced++;
      } catch (_) {
        // Leave it queued for the next sync attempt.
      }
    }
    return synced;
  }

  Future<Account> createAccount(AccountCreate account) async {
    try {
      final response = await _client.post(
        Uri.parse('$_baseUrl/accounts/'),
        headers: await _getHeaders(),
        body: json.encode(account.toJson()),
      );
      if (response.statusCode == 201) {
        final saved = Account.fromJson(json.decode(response.body) as Map<String, dynamic>);
        await _localDb.insertAccount(saved, isSynced: true);
        return saved;
      }
      throw Exception('Failed to create account: ${response.body}');
    } on OfflineException {
      // Offline: keep the account on-device; SyncService will replay it.
      final local = Account(
        accountNumber: account.accountNumber,
        accountName: account.accountName,
        accountType: account.accountType,
        normalBalance: ['Asset', 'Expense'].contains(account.accountType) ? 'debit' : 'credit',
        description: account.description,
        createdAt: DateTime.now(),
        updatedAt: DateTime.now(),
      );
      await _localDb.insertAccount(local, isSynced: false);
      return local;
    }
  }

  /// Pushes a locally-created (unsynced) account to the server. Used by
  /// SyncService; [bookId] restores the Book the account was created in.
  Future<void> pushAccountToServer(Account account, {String? bookId}) async {
    final headers = await _getHeaders();
    if (bookId != null) headers['X-Book-ID'] = bookId;
    final response = await _client.post(
      Uri.parse('$_baseUrl/accounts/'),
      headers: headers,
      body: json.encode(AccountCreate(
        accountNumber: account.accountNumber,
        accountName: account.accountName,
        accountType: account.accountType,
        currentBalance: 0,
        description: account.description,
      ).toJson()),
    );
    if (response.statusCode == 201 || response.statusCode == 200) {
      await _localDb.markAccountAsSynced(account.accountNumber);
    } else if (response.statusCode == 409) {
      // Account already exists server-side (e.g. replay after a lost
      // response): treat as synced, never duplicate.
      await _localDb.markAccountAsSynced(account.accountNumber);
    } else {
      throw Exception('Failed to sync account: ${response.body}');
    }
  }

  /// Network-first account list with offline cache fallback.
  Future<List<Account>> getAllAccounts() async {
    try {
      final response = await _client.get(
        Uri.parse('$_baseUrl/accounts/'),
        headers: await _getHeaders(),
      );
      if (response.statusCode == 200) {
        final List<dynamic> body = json.decode(response.body);
        final accounts = body
            .map((model) => Account.fromJson(model as Map<String, dynamic>))
            .toList();
        await _localDb.deleteAllAccounts();
        for (final account in accounts) {
          await _localDb.insertAccount(account);
        }
        return accounts;
      }
      throw Exception('Failed to get all accounts: ${response.body}');
    } on OfflineException {
      return _localDb.getAccounts();
    }
  }

  Future<Account> getAccount(String accountNumber) async {
    final response = await _client.get(
      Uri.parse('$_baseUrl/accounts/$accountNumber'),
      headers: await _getHeaders(),
    );
    if (response.statusCode == 200) {
      return Account.fromJson(json.decode(response.body) as Map<String, dynamic>);
    }
    throw Exception('Failed to get account: ${response.body}');
  }

  // ------------------------------------------------------------------
  // Financial Reports (network-only; they are computed server-side).
  // ------------------------------------------------------------------

  Future<TrialBalance> getTrialBalance({DateTime? asOfDate}) async {
    final query = asOfDate != null ? '?as_of_date=${asOfDate.toIso8601String()}' : '';
    final response = await _client.get(
      Uri.parse('$_baseUrl/trial-balance/$query'),
      headers: await _getHeaders(),
    );
    if (response.statusCode == 200) {
      return TrialBalance.fromJson(json.decode(response.body) as Map<String, dynamic>);
    }
    throw Exception('Failed to get trial balance: ${response.body}');
  }

  Future<IncomeStatement> getIncomeStatement(DateTime startDate, DateTime endDate) async {
    final response = await _client.get(
      Uri.parse('$_baseUrl/income-statement/'
          '?start_date=${startDate.toIso8601String()}'
          '&end_date=${endDate.toIso8601String()}'),
      headers: await _getHeaders(),
    );
    if (response.statusCode == 200) {
      return IncomeStatement.fromJson(json.decode(response.body) as Map<String, dynamic>);
    }
    throw Exception('Failed to get income statement: ${response.body}');
  }

  Future<BalanceSheet> getBalanceSheet(DateTime asOfDate) async {
    final response = await _client.get(
      Uri.parse('$_baseUrl/balance-sheet/?as_of_date=${asOfDate.toIso8601String()}'),
      headers: await _getHeaders(),
    );
    if (response.statusCode == 200) {
      return BalanceSheet.fromJson(json.decode(response.body) as Map<String, dynamic>);
    }
    throw Exception('Failed to get balance sheet: ${response.body}');
  }

  /// Ledger account balance for a single account. Maps the backend's
  /// LedgerReport shape into the mobile LedgerAccountBalance view model.
  Future<LedgerAccountBalance> getLedgerAccountBalance(String accountNumber) async {
    final response = await _client.get(
      Uri.parse('$_baseUrl/ledgers/$accountNumber'),
      headers: await _getHeaders(),
    );
    if (response.statusCode == 200) {
      final data = json.decode(response.body) as Map<String, dynamic>;
      return LedgerAccountBalance.fromJson(data);
    }
    throw Exception('Failed to get ledger balance: ${response.body}');
  }
}
