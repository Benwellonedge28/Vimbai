// mobile-client/lib/services/sync_service.dart
//
// Periodic background sync between the local SQLite store and the
// backend services. Pushes locally-created records first (so nothing
// is lost), then pulls fresh server data to refresh the offline cache.

import 'dart:async';
import 'package:vimbai_mobile_client/local_db/database_helper.dart';
import 'package:vimbai_mobile_client/services/accounting_api_service.dart';
import 'package:vimbai_mobile_client/services/multimodal_api_service.dart';
import 'package:vimbai_mobile_client/models/multimodal_models.dart';

class SyncService {
  final DatabaseHelper _localDb = DatabaseHelper();
  final AccountingApiService _accountingApiService = AccountingApiService();
  final MultimodalApiService _multimodalApiService = MultimodalApiService();

  Timer? _syncTimer;
  bool _isSyncing = false;

  static const Duration _syncInterval = Duration(minutes: 5);

  /// Starts the periodic sync timer.
  void startPeriodicSync() {
    _syncTimer?.cancel();
    _syncTimer = Timer.periodic(_syncInterval, (_) => syncAll());
  }

  /// Stops the periodic sync timer.
  void stopPeriodicSync() {
    _syncTimer?.cancel();
    _syncTimer = null;
  }

  /// Whether a sync cycle is currently running.
  bool get isSyncing => _isSyncing;

  /// Runs a full push-then-pull sync cycle. Safe to call repeatedly;
  /// concurrent cycles are ignored while one is in flight.
  Future<void> syncAll() async {
    if (_isSyncing) return;
    _isSyncing = true;
    try {
      await _syncAccountingData();
      await _syncMultimodalData();
    } catch (e) {
      // Sync failures are expected while offline; the next cycle retries.
      print('Sync cycle skipped: $e');
    } finally {
      _isSyncing = false;
    }
  }

  Future<void> _syncAccountingData() async {
    // 1. Push local-only journal entries created while offline.
    final unsynced = await _localDb.getUnsyncedJournalEntries();
    for (final entry in unsynced) {
      try {
        await _accountingApiService.pushJournalEntryToServer(entry);
      } catch (e) {
        print('Failed to push journal entry ${entry.id}: $e');
      }
    }

    // 2. Pull remote journal entries (also refreshes the local cache).
    try {
      await _accountingApiService.getJournalEntries();
    } catch (e) {
      print('Failed to pull journal entries: $e');
    }
  }

  Future<void> _syncMultimodalData() async {
    final unsyncedTasks = await _localDb.getUnsyncedMultimodalTasks();
    for (final task in unsyncedTasks) {
      try {
        await _multimodalApiService.createTask(MultimodalProcessingTaskCreate(
          userId: task.userId,
          inputType: task.inputType,
          inputUrl: task.dataUrl,
          inputRawText: task.rawText,
          metadata: task.metadata,
        ));
        await _localDb.markMultimodalTaskAsSynced(task.id);
      } catch (e) {
        print('Failed to push multimodal task ${task.id}: $e');
      }
    }
  }

  void dispose() {
    stopPeriodicSync();
  }
}
