// mobile-client/lib/local_db/local_database.dart

import 'package:vimbai_mobile_client/models/accounting_models.dart'; // Import Accounting models
import 'package:vimbai_mobile_client/models/multimodal_models.dart'; // Import Multimodal models

// Assume a local database package like Isar, Hive, or Sqflite is used.
// This abstract class defines the interface for local data access.

abstract class LocalDatabase {
  Future<void> initialize();

  // --- Journal Entry Operations ---
  Future<List<JournalEntryInDB>> getUnsyncedJournalEntries();
  Future<JournalEntryInDB?> getJournalEntry(String id);
  Future<void> saveJournalEntry(JournalEntryInDB entry);
  Future<void> markJournalEntryAsSynced(String id);
  // Future<void> deleteJournalEntry(String id);

  // --- Account Operations ---
  Future<List<AccountInDB>> getAllAccounts();
  Future<AccountInDB?> getAccount(String accountNumber);
  Future<void> saveAccount(AccountInDB account);
  // Future<void> deleteAccount(String accountNumber);

  // --- Multimodal Task Operations ---
  Future<List<MultimodalProcessingTaskInDB>> getUnsyncedMultimodalTasks();
  Future<MultimodalProcessingTaskInDB?> getMultimodalTask(String id);
  Future<void> saveMultimodalTask(MultimodalProcessingTaskInDB task);
  Future<void> markMultimodalTaskAsSynced(String id);
  // Future<void> deleteMultimodalTask(String id);

  // --- User Correction Operations ---
  Future<List<UserCorrectionInDB>> getUnsyncedUserCorrections();
  Future<UserCorrectionInDB?> getUserCorrection(String id);
  Future<void> saveUserCorrection(UserCorrectionInDB correction);
  Future<void> markUserCorrectionAsSynced(String id);
  Future<void> markAccountAsSynced(String accountNumber);
  // Future<void> deleteUserCorrection(String id);

  // Add methods for other entity types (e.g., BankConnection, etc.)
}

// Concrete (mock) implementation for demonstration purposes
class LocalDatabaseImpl implements LocalDatabase {
  final Map<String, JournalEntryInDB> _journalEntries = {};
  final Map<String, AccountInDB> _accounts = {};
  final Map<String, MultimodalProcessingTaskInDB> _multimodalTasks = {};
  final Map<String, UserCorrectionInDB> _userCorrections = {};

  // For simplicity, a local database doesn't manage synced status directly,
  // but rather is told to mark items as synced by the SyncService.
  // In a real implementation, entities would have an 'isSynced' flag.

  @override
  Future<void> initialize() async {
    print('Initializing LocalDatabase...');
    // Simulate async initialization
    await Future.delayed(Duration(milliseconds: 100));
    print('LocalDatabase initialized.');
  }

  // --- Journal Entry Operations ---
  @override
  Future<List<JournalEntryInDB>> getUnsyncedJournalEntries() async {
    // In a real DB, filter by isSynced == false
    return _journalEntries.values.toList();
  }

  @override
  Future<JournalEntryInDB?> getJournalEntry(String id) async {
    return _journalEntries[id];
  }

  @override
  Future<void> saveJournalEntry(JournalEntryInDB entry) async {
    _journalEntries[entry.id] = entry;
    print('Saved Journal Entry locally: ${entry.id}');
  }

  @override
  Future<void> markJournalEntryAsSynced(String id) async {
    // In a real DB, update 'isSynced' flag to true
    if (_journalEntries.containsKey(id)) {
      // Example: Create a new instance with isSynced = true if your model supports it
      // For now, just a print statement.
      print('Marked Journal Entry ${id} as synced.');
    }
  }

  // --- Account Operations ---
  @override
  Future<List<AccountInDB>> getAllAccounts() async {
    return _accounts.values.toList();
  }

  @override
  Future<AccountInDB?> getAccount(String accountNumber) async {
    return _accounts[accountNumber];
  }

  @override
  Future<void> saveAccount(AccountInDB account) async {
    _accounts[account.accountNumber] = account; // Using account number as key
    print('Saved Account locally: ${account.accountNumber}');
  }

  @override
  Future<void> markAccountAsSynced(String accountNumber) async {
    // In a real DB, update 'isSynced' flag to true
    if (_accounts.containsKey(accountNumber)) {
      print('Marked Account ${accountNumber} as synced.');
    }
  }

  // --- Multimodal Task Operations ---
  @override
  Future<List<MultimodalProcessingTaskInDB>> getUnsyncedMultimodalTasks() async {
    return _multimodalTasks.values.toList();
  }

  @override
  Future<MultimodalProcessingTaskInDB?> getMultimodalTask(String id) async {
    return _multimodalTasks[id];
  }

  @override
  Future<void> saveMultimodalTask(MultimodalProcessingTaskInDB task) async {
    _multimodalTasks[task.id] = task;
    print('Saved Multimodal Task locally: ${task.id}');
  }

  @override
  Future<void> markMultimodalTaskAsSynced(String id) async {
    if (_multimodalTasks.containsKey(id)) {
      print('Marked Multimodal Task ${id} as synced.');
    }
  }

  // --- User Correction Operations ---
  @override
  Future<List<UserCorrectionInDB>> getUnsyncedUserCorrections() async {
    return _userCorrections.values.toList();
  }

  @override
  Future<UserCorrectionInDB?> getUserCorrection(String id) async {
    return _userCorrections[id];
  }

  @override
  Future<void> saveUserCorrection(UserCorrectionInDB correction) async {
    _userCorrections[correction.id] = correction;
    print('Saved User Correction locally: ${correction.id}');
  }

  @override
  Future<void> markUserCorrectionAsSynced(String id) async {
    if (_userCorrections.containsKey(id)) {
      print('Marked User Correction ${id} as synced.');
    }
  }
}
