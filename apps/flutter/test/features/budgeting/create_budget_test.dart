import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;
import 'package:sqflite_common_ffi/sqflite_ffi.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/local_db/database_helper.dart';
import 'package:vimbai_mobile_client/features/budgeting/data/finance_api_service.dart';
import 'package:vimbai_mobile_client/features/budgeting/models/finance_models.dart';
import 'package:vimbai_mobile_client/features/budgeting/presentation/budget_form_page.dart';

/// The service's save path, with only the store kept: nothing here reaches the
/// engine, because a budget lives in the phone's own store.
class _SavingService extends FinanceApiService {
  _SavingService(this.saved);
  final List<List<BudgetItemCreate>> saved;

  @override
  Future<Budget> createBudget(
    BudgetCreate budgetCreate, {
    List<BudgetItemCreate> items = const [],
    bool isOffline = false,
  }) async {
    saved.add(items);
    return super.createBudget(budgetCreate, items: items, isOffline: true);
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() {
    sqfliteFfiInit();
    databaseFactory = databaseFactoryFfi;
    // connectivity_plus is a plugin, so a test has to answer for it: the save
    // asks whether the phone is online, and an unanswered channel throws.
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('dev.fluttercommunity.plus/connectivity'),
          (call) async => <String>['wifi'],
        );
    databaseFactory.setDatabasesPath(
      p.join(
        Directory.systemTemp.createTempSync('vimbai_budget_store').path,
        '',
      ),
    );
  });

  BudgetCreate header(String name) => BudgetCreate(
    name: name,
    startDate: DateTime(2026, 11, 1),
    endDate: DateTime(2026, 11, 30),
    currency: 'USD',
  );

  test('a budget is saved with the lines it was given', () async {
    // The form collects a figure per category, and the save used to write only
    // the header: the person was told it worked while everything they typed was
    // thrown away.
    final service = FinanceApiService();
    final budget = await service.createBudget(
      header('Household November'),
      items: [
        BudgetItemCreate(
          category: 'Rent',
          accountNumber: '6100',
          budgetedAmount: ExactAmount.parse('500'),
        ),
        BudgetItemCreate(
          category: 'Groceries',
          accountNumber: '6200',
          budgetedAmount: ExactAmount.parse('250.50'),
        ),
      ],
      isOffline: true,
    );

    final saved = (await service.getBudgets()).firstWhere(
      (b) => b.id == budget.id,
    );
    expect(saved.items.length, 2);
    expect(saved.items.first.category, 'Rent');
    expect(saved.items.first.budgetedAmount, ExactAmount.parse('500'));
    expect(saved.items.last.budgetedAmount, ExactAmount.parse('250.50'));
    // each line belongs to the budget it came in with, not to some other one
    expect(saved.items.every((i) => i.budgetId == budget.id), isTrue);
  });

  test('a thirty-digit budget line survives the phone store exactly', () async {
    // budgeted_amount was a REAL column, which rewrites anything a double
    // cannot hold - so a large budget came back a different figure.
    final amount = ExactAmount.parse('123456789012345678901234567890.123456');
    final service = FinanceApiService();
    final budget = await service.createBudget(
      header('Very large budget'),
      items: [
        BudgetItemCreate(
          category: 'Plant',
          accountNumber: '1500',
          budgetedAmount: amount,
        ),
      ],
      isOffline: true,
    );

    final saved = (await service.getBudgets()).firstWhere(
      (b) => b.id == budget.id,
    );
    expect(saved.items.single.budgetedAmount, amount);
    expect(
      saved.items.single.budgetedAmount.toLedgerString(),
      amount.toLedgerString(),
    );
  });

  test('an upgrade from the old table carries its figures across', () async {
    // A phone that already held budgets stored them as REAL. The upgrade
    // rebuilds the table, and the figures it held must still read back.
    final dir = Directory.systemTemp.createTempSync('vimbai_budget_upgrade');
    final path = p.join(dir.path, 'vimbai_offline.db');
    await databaseFactory.deleteDatabase(path);

    final old = await databaseFactory.openDatabase(
      path,
      options: OpenDatabaseOptions(
        version: 5,
        onCreate: (db, _) async {
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
          await db.execute('''
                CREATE TABLE budget_items(
                  id TEXT PRIMARY KEY,
                  budget_id TEXT,
                  category TEXT,
                  account_number TEXT,
                  budgeted_amount REAL,
                  budget_type TEXT,
                  created_at TEXT,
                  updated_at TEXT
                )
              ''');
        },
      ),
    );
    await old.insert('budgets', {
      'id': 'b-old',
      'name': 'From an older phone',
      'start_date': DateTime(2026, 11, 1).toIso8601String(),
      'end_date': DateTime(2026, 11, 30).toIso8601String(),
      'currency': 'USD',
    });
    await old.insert('budget_items', {
      'id': 'i-old',
      'budget_id': 'b-old',
      'category': 'Rent',
      'account_number': '6100',
      'budgeted_amount': 750.25,
      'budget_type': 'expense',
    });
    await old.close();

    final helper = DatabaseHelper();
    final reopened = await databaseFactory.openDatabase(
      path,
      options: OpenDatabaseOptions(version: 6),
    );
    // opening through the helper would go to its own file, so the upgrade step
    // is applied to this file the same way the helper applies it
    await helper.upgradeForTest(reopened, 5);
    final rows = await reopened.query('budget_items');
    await reopened.close();

    expect(rows.single['category'], 'Rent');
    expect(
      ExactAmount.fromJson(rows.single['budgeted_amount']),
      ExactAmount.parse('750.25'),
    );
  });

  testWidgets('what is filled in on the form is what gets saved', (
    tester,
  ) async {
    final saved = <List<BudgetItemCreate>>[];
    await tester.pumpWidget(
      MaterialApp(home: BudgetFormPage(apiService: _SavingService(saved))),
    );
    await tester.pumpAndSettle();

    await tester.enterText(
      find.widgetWithText(TextFormField, 'Budget Name'),
      'Farming season',
    );
    await tester.tap(find.widgetWithText(ElevatedButton, 'Add Item'));
    await tester.pumpAndSettle();

    // named fields, not positions: the form carries a name, a currency and a
    // description of its own before the line the person just added
    await tester.ensureVisible(find.widgetWithText(TextFormField, 'Category'));
    await tester.enterText(
      find.widgetWithText(TextFormField, 'Category'),
      'Seed',
    );
    await tester.pumpAndSettle();
    final amountField = find.widgetWithText(TextFormField, 'Budgeted Amount');
    await tester.ensureVisible(amountField);
    await tester.enterText(amountField, '1250.75');
    await tester.pumpAndSettle();

    final create = find.widgetWithText(ElevatedButton, 'Create Budget');
    await tester.ensureVisible(create);
    await tester.tap(create);
    await tester.pumpAndSettle();

    expect(saved.single.length, 1);
    expect(saved.single.single.category, 'Seed');
    expect(saved.single.single.budgetedAmount, ExactAmount.parse('1250.75'));
  });

  testWidgets('an amount that is not a figure is refused, not saved as zero', (
    tester,
  ) async {
    final saved = <List<BudgetItemCreate>>[];
    await tester.pumpWidget(
      MaterialApp(home: BudgetFormPage(apiService: _SavingService(saved))),
    );
    await tester.pumpAndSettle();

    await tester.enterText(
      find.widgetWithText(TextFormField, 'Budget Name'),
      'Typo budget',
    );
    await tester.tap(find.widgetWithText(ElevatedButton, 'Add Item'));
    await tester.pumpAndSettle();

    await tester.ensureVisible(find.widgetWithText(TextFormField, 'Category'));
    await tester.enterText(
      find.widgetWithText(TextFormField, 'Category'),
      'Seed',
    );
    final amountField = find.widgetWithText(TextFormField, 'Budgeted Amount');
    await tester.ensureVisible(amountField);
    await tester.enterText(amountField, 'twelve');
    await tester.pumpAndSettle();

    final create = find.widgetWithText(ElevatedButton, 'Create Budget');
    await tester.ensureVisible(create);
    await tester.tap(create);
    await tester.pumpAndSettle();

    expect(saved, isEmpty);
    expect(find.text('Enter a valid amount'), findsOneWidget);
  });
}
