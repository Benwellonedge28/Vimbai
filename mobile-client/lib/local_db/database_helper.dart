import 'package:sqflite/sqflite.dart';
import 'package:path/path.dart';
import 'package:finacc_mobile_client/models/accounting_models.dart'; // Import accounting models
import 'package:finacc_mobile_client/models/finance_models.dart'; // Import finance models
import 'package:finacc_mobile_client/models/multimodal_models.dart'; // NEW: Import multimodal models
import 'package:uuid/uuid.dart';

class DatabaseHelper {
  static final DatabaseHelper _instance = DatabaseHelper._internal();
  static Database? _database;
  final Uuid uuid = Uuid();

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
      version: 2, // NEW: Increment database version
      onCreate: _onCreate,
      onUpgrade: _onUpgrade,
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

    // Create Budgets table
    await db.execute('''
          CREATE TABLE budgets(
            id TEXT PRIMARY KEY,
            name TEXT,
            start_date TEXT,
            end_date TEXT,
            currency TEXT,
            description TEXT,
            is_synced INTEGER DEFAULT 0,
            created_at TEXT,
            updated_at TEXT
          )
        ''');

    // Create BudgetItems table
    await db.execute('''
          CREATE TABLE budget_items(
            id TEXT PRIMARY KEY,
            budget_id TEXT,
            category TEXT,
            account_number TEXT,
            budgeted_amount REAL,
            budget_type TEXT,
            created_at TEXT,
            updated_at TEXT,
            FOREIGN KEY (budget_id) REFERENCES budgets(id) ON DELETE CASCADE
          )
        ''');
    
    // NEW: Create MultimodalTasks table
    await db.execute('''
          CREATE TABLE multimodal_tasks(
            id TEXT PRIMARY KEY,
            input_type TEXT,
            data TEXT, -- Base64 encoded file data or URL
            source_context TEXT,
            is_synced INTEGER DEFAULT 0,
            created_at TEXT,
            updated_at TEXT
          )
        ''');
  }

  Future<void> _onUpgrade(Database db, int oldVersion, int newVersion) async {
    if (oldVersion < 2) {
      // Migrate from version 1 to 2: Add multimodal_tasks table
      await db.execute('''
            CREATE TABLE multimodal_tasks(
              id TEXT PRIMARY KEY,
              input_type TEXT,
              data TEXT, -- Base64 encoded file data or URL
              source_context TEXT,
              is_synced INTEGER DEFAULT 0,
              created_at TEXT,
              updated_at TEXT
            )
          ''');
    }
    // Add future migrations here
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
      return Account.fromJson(maps[i]);
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
        return JournalLine.fromJson(lineMaps[i]);
      });
      unsyncedEntries.add(JournalEntry.fromJson({...entryMap, 'lines': lines}));
    }\n    return unsyncedEntries;
  }

  Future<int> markJournalEntryAsSynced(String entryId) async {
    final db = await database;
    return await db.update('journal_entries', {'is_synced': 1, 'updated_at': DateTime.now().toIso8601String()},
        where: 'id = ?', whereArgs: [entryId]);
  }

  // --- Budget CRUD ---
  Future<int> insertBudget(Budget budget, {bool isSynced = false}) async {
    final db = await database;
    final budgetId = budget.id ?? uuid.v4();
    final result = await db.insert('budgets', {
      'id': budgetId,
      'name': budget.name,
      'start_date': budget.startDate.toIso8601String(),
      'end_date': budget.endDate.toIso8601String(),
      'currency': budget.currency,
      'description': budget.description,
      'is_synced': isSynced ? 1 : 0,
      'created_at': DateTime.now().toIso8601String(),
      'updated_at': DateTime.now().toIso8601String(),
    }, conflictAlgorithm: ConflictAlgorithm.replace);

    // Insert budget items
    for (var item in budget.items) {
      await insertBudgetItem(budgetId, item, isSynced: isSynced);
    }
    return result;
  }

  Future<Budget?> getBudget(String budgetId) async {
    final db = await database;
    final List<Map<String, dynamic>> budgetMaps = await db.query('budgets', where: 'id = ?', whereArgs: [budgetId]);
    if (budgetMaps.isNotEmpty) {
      final budgetMap = budgetMaps.first;
      final items = await getBudgetItemsForBudget(budgetId);
      return Budget.fromJson({...budgetMap, 'items': items});
    }
    return null;
  }

  Future<List<Budget>> getBudgetsFromLocal() async {
    final db = await database;
    final List<Map<String, dynamic>> budgetMaps = await db.query('budgets');
    List<Budget> budgets = [];
    for (var budgetMap in budgetMaps) {
      final items = await getBudgetItemsForBudget(budgetMap['id']);
      budgets.add(Budget.fromJson({...budgetMap, 'items': items}));
    }
    return budgets;
  }

  Future<List<Budget>> getUnsyncedBudgets() async {
    final db = await database;
    final List<Map<String, dynamic>> budgetMaps = await db.query('budgets', where: 'is_synced = ?', whereArgs: [0]);
    List<Budget> unsyncedBudgets = [];
    for (var budgetMap in budgetMaps) {
      final items = await getBudgetItemsForBudget(budgetMap['id']);
      unsyncedBudgets.add(Budget.fromJson({...budgetMap, 'items': items}));
    })
    return unsyncedBudgets;
  }

  Future<int> markBudgetAsSynced(String budgetId) async {
    final db = await database;
    return await db.update('budgets', {'is_synced': 1, 'updated_at': DateTime.now().toIso8601String()},
        where: 'id = ?', whereArgs: [budgetId]);
  }

  Future<int> deleteAllBudgets() async {
    final db = await database;
    await db.delete('budget_items'); // Delete items first due to FK
    return await db.delete('budgets');
  }


  // --- BudgetItem CRUD ---
  Future<int> insertBudgetItem(String budgetId, BudgetItem item, {bool isSynced = false}) async {
    final db = await database;
    return await db.insert('budget_items', {
      'id': item.id ?? uuid.v4(),
      'budget_id': budgetId,
      'category': item.category,
      'account_number': item.accountNumber,
      'budgeted_amount': item.budgetedAmount,
      'budget_type': item.budgetType,
      'created_at': DateTime.now().toIso8601String(),
      'updated_at': DateTime.now().toIso8601String(),
    }, conflictAlgorithm: ConflictAlgorithm.replace);
  }

  Future<List<BudgetItem>> getBudgetItemsForBudget(String budgetId) async {
    final db = await database;
    final List<Map<String, dynamic>> maps = await db.query(
      'budget_items',
      where: 'budget_id = ?',
      whereArgs: [budgetId],
    );
    return List.generate(maps.length, (i) {
      return BudgetItem.fromJson(maps[i]);
    });
  }

  Future<int> updateBudgetItemLocal(BudgetItem item) async {
    final db = await database;
    return await db.update('budget_items', {
      'category': item.category,
      'account_number': item.accountNumber,
      'budgeted_amount': item.budgetedAmount,
      'budget_type': item.budgetType,
      'updated_at': DateTime.now().toIso8601String(),
    }, where: 'id = ?', whereArgs: [item.id]);
  }

  Future<int> deleteBudgetItemLocal(String itemId) async {
    final db = await database;
    return await db.delete('budget_items', where: 'id = ?', whereArgs: [itemId]);
  }

  // NEW: --- Multimodal Task CRUD ---
  Future<int> insertMultimodalTask(MultimodalInput task, {bool isSynced = false}) async {
    final db = await database;
    final taskId = uuid.v4(); // Always generate a new local ID for pending tasks
    final result = await db.insert('multimodal_tasks', {
      'id': taskId,
      'input_type': task.inputType,
      'data': task.data, // This should be base64 for files or raw URL for URL inputs
      'source_context': task.sourceContext,
      'is_synced': isSynced ? 1 : 0,
      'created_at': DateTime.now().toIso8601String(),
      'updated_at': DateTime.now().toIso8601String(),
    });
    return result;
  }

  Future<List<MultimodalInput>> getUnsyncedMultimodalTasks() async {
    final db = await database;
    final List<Map<String, dynamic>> taskMaps = await db.query('multimodal_tasks', where: 'is_synced = ?', whereArgs: [0]);
    return List.generate(taskMaps.length, (i) {
      return MultimodalInput.fromJson(taskMaps[i]);
    });
  }

  Future<int> markMultimodalTaskAsSynced(String taskId) async {
    final db = await database;
    return await db.update('multimodal_tasks', {'is_synced': 1, 'updated_at': DateTime.now().toIso8601String()},
        where: 'id = ?', whereArgs: [taskId]);
  }
}
