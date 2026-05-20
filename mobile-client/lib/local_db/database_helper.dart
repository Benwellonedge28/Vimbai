import 'package:sqflite/sqflite.dart';
import 'package:path/path.dart';
import 'package:finacc_mobile_client/models/accounting_models.dart'; // Import accounting models
import 'package:uuid/uuid.dart'; // NEW

class DatabaseHelper {
  static final DatabaseHelper _instance = DatabaseHelper._internal();
  static Database? _database;
  final Uuid uuid = Uuid(); // NEW

  factory DatabaseHelper() {
    return _instance;
  }

  DatabaseHelper._internal();

  Future<Database> get database async {
    if (_database != null) return _database!;
    _database = await _initDatabase();
    return _database!;
  }

  Future<Database> _initDatabase() async {
    String path = join(await getDatabasesPath(), 'finacc_offline.db');
    return await openDatabase(
      path,
      version: 1,
      onCreate: _onCreate,
    );
  }

  Future<void> _onCreate(Database db, int version) async {
    // Create Account table
    await db.execute('''
          CREATE TABLE accounts(
            id TEXT PRIMARY KEY,
            account_number TEXT UNIQUE,
            account_name TEXT,
            account_type TEXT,
            normal_balance TEXT,
            description TEXT,
            parent_account_number TEXT,
            created_at TEXT,
            updated_at TEXT
          )
        ''');

    // Create JournalEntry table (for offline entries to be synced)
    await db.execute('''
          CREATE TABLE journal_entries(
            id TEXT PRIMARY KEY,
            entry_date TEXT,
            description TEXT,
            reference_number TEXT,
            source_module TEXT,
            is_synced INTEGER DEFAULT 0,
            created_at TEXT,
            updated_at TEXT
          )
        ''');

    // Create JournalLine table
    await db.execute('''
          CREATE TABLE journal_lines(
            id TEXT PRIMARY KEY,
            journal_entry_id TEXT,
            account_number TEXT,
            debit REAL,
            credit REAL,
            description TEXT,
            FOREIGN KEY (journal_entry_id) REFERENCES journal_entries(id) ON DELETE CASCADE
          )
        ''');
  }

  // --- Account CRUD ---
  Future<int> insertAccount(Account account) async {
    final db = await database;
    return await db.insert('accounts', {
      'id': account.id,
      'account_number': account.accountNumber,
      'account_name': account.accountName,
      'account_type': account.accountType,
      'normal_balance': account.normalBalance,
      'description': account.description,
      'parent_account_number': account.parentAccountNumber,
      'created_at': account.createdAt.toIso8601String(),
      'updated_at': account.updatedAt.toIso8601String(),
    }, conflictAlgorithm: ConflictAlgorithm.replace);
  }

  Future<List<Account>> getAccounts() async {
    final db = await database;
    final List<Map<String, dynamic>> maps = await db.query('accounts');
    return List.generate(maps.length, (i) {
      return Account(
        id: maps[i]['id'],
        accountNumber: maps[i]['account_number'],
        accountName: maps[i]['account_name'],
        accountType: maps[i]['account_type'],
        normalBalance: maps[i]['normal_balance'],
        description: maps[i]['description'],
        parentAccountNumber: maps[i]['parent_account_number'],
        createdAt: DateTime.parse(maps[i]['created_at']),
        updatedAt: DateTime.parse(maps[i]['updated_at']),
      );
    });
  }

  Future<int> deleteAllAccounts() async {
    final db = await database;
    return await db.delete('accounts');
  }

  // --- Journal Entry CRUD ---
  Future<int> insertJournalEntry(JournalEntry entry, {bool isSynced = false}) async {
    final db = await database;
    final entryId = entry.id ?? uuid.v4(); // Generate UUID if not already present
    final result = await db.insert('journal_entries', {
      'id': entryId,
      'entry_date': entry.entryDate.toIso8601String(),
      'description': entry.description,
      'reference_number': entry.referenceNumber,
      'source_module': entry.sourceModule,
      'is_synced': isSynced ? 1 : 0,
      'created_at': DateTime.now().toIso8601String(),
      'updated_at': DateTime.now().toIso8601String(),
    });

    // Insert journal lines
    for (var line in entry.lines) {
      await db.insert('journal_lines', {
        'id': uuid.v4(), // Unique ID for each line
        'journal_entry_id': entryId,
        'account_number': line.accountNumber,
        'debit': line.debit,
        'credit': line.credit,
        'description': line.description,
      });
    }
    return result;
  }

  Future<List<JournalEntry>> getUnsyncedJournalEntries() async {
    final db = await database;
    final List<Map<String, dynamic>> entryMaps = await db.query('journal_entries', where: 'is_synced = ?', whereArgs: [0]);
    List<JournalEntry> unsyncedEntries = [];

    for (var entryMap in entryMaps) {
      final List<Map<String, dynamic>> lineMaps = await db.query('journal_lines', where: 'journal_entry_id = ?', whereArgs: [entryMap['id']]);
      List<JournalLine> lines = List.generate(lineMaps.length, (i) {
        return JournalLine(
          accountNumber: lineMaps[i]['account_number'],
          debit: lineMaps[i]['debit'],
          credit: lineMaps[i]['credit'],
          description: lineMaps[i]['description'],
        );
      });
      unsyncedEntries.add(JournalEntry(
        id: entryMap['id'],
        entryDate: DateTime.parse(entryMap['entry_date']),
        description: entryMap['description'],
        referenceNumber: entryMap['reference_number'],
        sourceModule: entryMap['source_module'],
        lines: lines,
        createdAt: DateTime.parse(entryMap['created_at']),
        updatedAt: DateTime.parse(entryMap['updated_at']),
      ));
    }
    return unsyncedEntries;
  }

  Future<int> markJournalEntryAsSynced(String entryId) async {
    final db = await database;
    return await db.update('journal_entries', {'is_synced': 1, 'updated_at': DateTime.now().toIso8601String()},
        where: 'id = ?', whereArgs: [entryId]);
  }
}
