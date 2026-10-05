// mobile-client/lib/services/sync_service.dart
//
// Periodic background sync between the local SQLite store and the
// backend services. Pushes locally-created records first (so nothing
// is lost), then pulls fresh server data to refresh the offline cache.

import 'dart:async';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:vimbai_mobile_client/local_db/database_helper.dart';
import 'package:vimbai_mobile_client/services/accounting_api_service.dart';
import 'package:vimbai_mobile_client/services/multimodal_api_service.dart';
import 'package:vimbai_mobile_client/models/multimodal_models.dart';

class SyncService {
  /// App-wide instance. Started after login, stopped on logout.
  static final SyncService instance = SyncService._();

  SyncService._();

  StreamSubscription<List<ConnectivityResult>>? _connectivitySub;

  final DatabaseHelper _localDb = DatabaseHelper();
  final AccountingApiService _accountingApiService = AccountingApiService();
  final MultimodalApiService _multimodalApiService = MultimodalApiService();

  Timer? _syncTimer;
  bool _isSyncing = false;

  static const Duration _syncInterval = Duration(minutes: 5);

  /// Starts syncing: an immediate cycle, a periodic timer, and a
  /// connectivity listener so the outbox flushes the moment the device
  /// regains network (not just on the next timer tick).
  void start() {
    _syncTimer?.cancel();
    _syncTimer = Timer.periodic(_syncInterval, (_) => syncAll());
    _connectivitySub?.cancel();
    _connectivitySub = Connectivity()
        .onConnectivityChanged
        .listen((results) {
      final online = results.any((r) => r != ConnectivityResult.none);
      if (online) syncAll();
    });
    syncAll(); // catch up immediately (e.g. app resumed, user re-logged-in)
  }

  /// Stops all syncing (logout).
  void stop() {
    _syncTimer?.cancel();
    _syncTimer = null;
    _connectivitySub?.cancel();
    _connectivitySub = null;
  }

  /// Records (journal entries + captures) waiting to sync. Used by the
  /// UI to show a pending-sync badge.
  Future<int> pendingCount() => _localDb.pendingSyncCount();

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
    // 1. Push accounts created offline first (entries reference them).
    final unsyncedAccounts = await _localDb.getUnsyncedAccounts();
    for (final ref in unsyncedAccounts) {
      try {
        await _accountingApiService.pushAccountToServer(ref.item, bookId: ref.bookId);
      } catch (e) {
        print('Failed to push account ${ref.item.accountNumber}: $e');
      }
    }

    // 2. Push local-only journal entries created while offline, replaying
    // each into the Book it was originally created in.
    final unsynced = await _localDb.getUnsyncedJournalEntriesWithBooks();
    for (final ref in unsynced) {
      try {
        await _accountingApiService.pushJournalEntryToServer(ref.item, bookId: ref.bookId);
      } catch (e) {
        print('Failed to push journal entry ${ref.item.id}: $e');
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
    stop();
  }
}
