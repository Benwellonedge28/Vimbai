// mobile-client/lib/services/sync_service.dart

import 'dart:async';
import 'package:finacc_mobile_client/local_db/local_database.dart'; // Assuming this exists
import 'package:finacc_mobile_client/services/auth_service.dart';
import 'package:finacc_mobile_client/services/accounting_api_service.dart';
import 'package:finacc_mobile_client/services/multimodal_api_service.dart';
// Import other API services as needed

class SyncService {
  final LocalDatabase _localDb;
  final AuthService _authService;
  final AccountingApiService _accountingApiService;
  final MultimodalApiService _multimodalApiService;
  // Add other API services

  Timer? _syncTimer;
  bool _isSyncing = false;

  // Configuration for sync
  static const Duration _syncInterval = Duration(minutes: 5); // Sync every 5 minutes

  SyncService({
    required LocalDatabase localDb,
    required AuthService authService,
    required AccountingApiService accountingApiService,
    required MultimodalApiService multimodalApiService,
    // Add other services
  }) : _localDb = localDb,
       _authService = authService,
       _accountingApiService = accountingApiService,
       _multimodalApiService = multimodalApiService;

  void startPeriodicSync() {
    _syncTimer?.cancel(); // Cancel any existing timer
    _syncTimer = Timer.periodic(_syncInterval, (timer) => syncAll());
    print('Periodic sync started. Interval: ${_syncInterval.inMinutes} minutes.');
  }

  void stopPeriodicSync() {
    _syncTimer?.cancel();
    _syncTimer = null;
    print('Periodic sync stopped.');
  }

  Future<void> syncAll() async {
    if (_isSyncing) {
      print('Sync already in progress. Skipping this cycle.');
      return;
    }
    if (!await _authService.isAuthenticated()) {
      print('User not authenticated. Skipping sync.');
      return;
    }

    _isSyncing = true;
    print('Starting all data synchronization...');

    try {
      await _syncAccountingData();
      await _syncMultimodalData();
      // Add other data sync methods here
      print('All data synchronization completed successfully.');
    } catch (e) {
      print('Error during full synchronization: $e');
      // Implement robust error reporting/retry mechanisms
    } finally {
      _isSyncing = false;
    }
  }

  Future<void> _syncAccountingData() async {
    print('Syncing Accounting data...');
    // 1. Push local changes (e.g., new journal entries created offline)
    final localJournalEntries = await _localDb.getUnsyncedJournalEntries();
    for (var entry in localJournalEntries) {
      try {
        await _accountingApiService.createJournalEntry(entry);
        await _localDb.markJournalEntryAsSynced(entry.id); // Assuming entry has an ID
        print('Pushed local Journal Entry: ${entry.id}');
      } catch (e) {
        print('Failed to push Journal Entry ${entry.id}: $e');
        // Handle conflicts or network issues: queue for retry, notify user
      }
    }

    // 2. Pull remote changes
    final remoteJournalEntries = await _accountingApiService.getAllJournalEntries();
    for (var remoteEntry in remoteJournalEntries) {
      final localEntry = await _localDb.getJournalEntry(remoteEntry.id);
      if (localEntry == null || remoteEntry.updatedAt.isAfter(localEntry.updatedAt)) {
        await _localDb.saveJournalEntry(remoteEntry); // Overwrite or insert
        print('Pulled remote Journal Entry: ${remoteEntry.id}');
      } else if (localEntry.updatedAt.isAfter(remoteEntry.updatedAt) && !localEntry.isSynced) {
        // Conflict: local is newer, remote is older, local not yet pushed
        // Implement conflict resolution logic here (e.g., last-write-wins, merge, user choice)
        print('Conflict detected for Journal Entry ${remoteEntry.id}. Local is newer.');
        // For now, let's assume local takes precedence if unsynced, and it will be pushed in next cycle.
      }
    }
    print('Accounting data sync finished.');
  }

  Future<void> _syncMultimodalData() async {
    print('Syncing Multimodal data...');
    // 1. Push local changes (e.g., new multimodal inputs, user corrections)
    final localTasks = await _localDb.getUnsyncedMultimodalTasks();
    for (var task in localTasks) {
      try {
        // Logic to decide between create or update
        if (task.status == MultimodalProcessingStatus.received) { // Or another indicator for new task
          await _multimodalApiService.createTask(MultimodalProcessingTaskCreate(
            userId: task.userId,
            inputType: task.inputType,
            inputUrl: task.inputUrl,
            inputRawText: task.inputRawText,
            metadata: task.metadata,
          ));
        } else { // Assume it's an update if already exists locally and is not 'received'
          await _multimodalApiService.updateTask(task.id, MultimodalProcessingTaskUpdate(
            status: task.status, // Update status, e.g., 'user_corrected'
            documentResult: task.documentResult, // Push user corrections
            // ... other fields from update model
          ));
        }
        await _localDb.markMultimodalTaskAsSynced(task.id);
        print('Pushed local Multimodal Task: ${task.id}');
      } catch (e) {
        print('Failed to push Multimodal Task ${task.id}: $e');
      }
    }

    final localCorrections = await _localDb.getUnsyncedUserCorrections();
    for (var correction in localCorrections) {
      try {
        await _multimodalApiService.submitUserCorrection(correction.taskId, correction);
        await _localDb.markUserCorrectionAsSynced(correction.id);
        print('Pushed local User Correction: ${correction.id}');
      } catch (e) {
        print('Failed to push User Correction ${correction.id}: $e');
      }
    }

    // 2. Pull remote changes
    final remoteTasks = await _multimodalApiService.getAllTasks();
    for (var remoteTask in remoteTasks) {
      final localTask = await _localDb.getMultimodalTask(remoteTask.id);
      if (localTask == null || remoteTask.updatedAt.isAfter(localTask.updatedAt)) {
        await _localDb.saveMultimodalTask(remoteTask);
        print('Pulled remote Multimodal Task: ${remoteTask.id}');
      } // Conflict resolution for multimodal tasks would also go here
    }
    print('Multimodal data sync finished.');
  }

  // Add other specific sync methods here

  // Optional: manual trigger
  Future<void> triggerManualSync() async {
    await syncAll();
  }
}
