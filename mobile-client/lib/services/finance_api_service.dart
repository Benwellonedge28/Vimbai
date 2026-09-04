import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/services/api_client.dart';
import 'package:vimbai_mobile_client/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/config.dart'; // For API URL
import 'package:vimbai_mobile_client/models/finance_models.dart'; // Import Finance Models
import 'package:connectivity_plus/connectivity_plus.dart'; // For offline detection
import 'package:vimbai_mobile_client/local_db/database_helper.dart'; // For local DB access
import 'package:uuid/uuid.dart'; // For generating UUIDs
import 'package:vimbai_mobile_client/services/book_context.dart';

class FinanceApiService {
  final ApiClient _client = ApiClient();
  final String _financeServiceUrl = '${AppConfig.apiUrl}/budgets'; // Use API Gateway path prefix for budgets
  final String _financialRatiosUrl = '${AppConfig.apiUrl}/financial-ratios'; // Use API Gateway path prefix for financial ratios

  final DatabaseHelper _dbHelper = DatabaseHelper();

  Future<Map<String, String>> _getHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer $token',
      ...BookContext.instance.headers(),
    };
  }

  // --- Budget API Calls with Offline Support ---
  Future<Budget> createBudget(BudgetCreate budgetCreate, {bool isOffline = false}) async {
    final connectivityResult = await (Connectivity().checkConnectivity());
    if (isOffline || connectivityResult == ConnectivityResult.none) {
      final localBudget = Budget(
        id: budgetCreate.id ?? const Uuid().v4(), // Ensure local ID
        name: budgetCreate.name,
        startDate: budgetCreate.startDate,
        endDate: budgetCreate.endDate,
        currency: budgetCreate.currency,
        description: budgetCreate.description,
        items: [], // Items will be added separately or synced later
        createdAt: DateTime.now(),
        updatedAt: DateTime.now(),
      );
      await _dbHelper.insertBudget(localBudget, isSynced: false);
      print('Offline: Stored budget locally: ${localBudget.name}');
      return localBudget;
    }

    // Try to send to remote
    try {
      final headers = await _getHeaders();
      final response = await _client.post(
        Uri.parse('$_financeServiceUrl/'),
        headers: headers,
        body: json.encode(budgetCreate.toJson()),
      );

      if (response.statusCode == 201) {
        print('Online: Successfully created budget remotely: ${budgetCreate.name}');
        return Budget.fromJson(json.decode(response.body));
      } else {
        print('Online: Failed to create budget remotely: ${response.statusCode} - ${response.body}');
        throw Exception('Failed to create budget: ${response.body}');
      }
    } catch (e) {
      print('Online: Error creating budget remotely, attempting local save: $e');
      final localBudget = Budget(
        id: budgetCreate.id ?? const Uuid().v4(), // Ensure local ID
        name: budgetCreate.name,
        startDate: budgetCreate.startDate,
        endDate: budgetCreate.endDate,
        currency: budgetCreate.currency,
        description: budgetCreate.description,
        items: [], // Items will be added separately or synced later
        createdAt: DateTime.now(),
        updatedAt: DateTime.now(),
      );
      await _dbHelper.insertBudget(localBudget, isSynced: false);
      throw Exception('Failed to create budget remotely, saved offline: $e');
    }
  }

  Future<List<Budget>> getBudgets({bool forceRemote = false}) async {
    final connectivityResult = await (Connectivity().checkConnectivity());
    if (!forceRemote && connectivityResult == ConnectivityResult.none) {
      print('Offline: Fetching budgets from local DB.');
      return await _dbHelper.getBudgetsFromLocal();
    }

    final headers = await _getHeaders();
    final response = await _client.get(Uri.parse('$_financeServiceUrl/'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> budgetsJson = json.decode(response.body);
      List<Budget> budgets = budgetsJson.map((json) => Budget.fromJson(json)).toList();
      // Store in local DB
      print('Online: Fetched budgets from remote, updating local DB.');
      await _dbHelper.deleteAllBudgets(); // Clear old budgets
      for (var budget in budgets) {
        await _dbHelper.insertBudget(budget); // Insert with items
      }
      return budgets;
    } else {
      print('Failed to load budgets: ${response.statusCode} - ${response.body}');
      throw Exception('Failed to load budgets: ${response.body}');
    }
  }

  Future<Budget> getBudgetById(String budgetId) async {
    final connectivityResult = await (Connectivity().checkConnectivity());
    if (connectivityResult == ConnectivityResult.none) {
      print('Offline: Attempting to fetch budget from local DB: $budgetId');
      final localBudget = await _dbHelper.getBudget(budgetId);
      if (localBudget != null) {
        return localBudget;
      }
      throw Exception('Budget $budgetId not found in local DB while offline.');
    }

    final headers = await _getHeaders();
    final response = await _client.get(Uri.parse('$_financeServiceUrl/$budgetId'), headers: headers);

    if (response.statusCode == 200) {
      final remoteBudget = Budget.fromJson(json.decode(response.body));
      // Update local DB with fresh data
      await _dbHelper.insertBudget(remoteBudget, isSynced: true);
      return remoteBudget;
    } else {
      throw Exception('Failed to load budget: ${response.body}');
    }
  }

  // Future<Budget> updateBudget(String budgetId, BudgetUpdate budget) async {
  //   // For now, update operations require online connection.
  //   // Offline updates would require more complex conflict resolution logic.
  //   final headers = await _getHeaders();
  //   final response = await _client.put(
  //     Uri.parse('$_financeServiceUrl/$budgetId'),
  //     headers: headers,
  //     body: json.encode(budget.toJson()),
  //   );

  //   if (response.statusCode == 200) {
  //     return Budget.fromJson(json.decode(response.body));
  //   } else {
  //     throw Exception('Failed to update budget: ${response.body}');
  //   }
  // }

  // Future<void> deleteBudget(String budgetId) async {
  //   // For now, delete operations require online connection.
  //   final headers = await _getHeaders();
  //   final response = await _client.delete(
  //     Uri.parse('$_financeServiceUrl/$budgetId'),
  //     headers: headers,
  //   );

  //   if (response.statusCode != 204) {
  //     throw Exception('Failed to delete budget: ${response.body}');
  //   }
  // }

  // --- Budget Item API Calls with Offline Support ---
  Future<BudgetItem> createBudgetItem(String budgetId, BudgetItemCreate itemCreate, {bool isOffline = false}) async {
    final connectivityResult = await (Connectivity().checkConnectivity());
    if (isOffline || connectivityResult == ConnectivityResult.none) {
      final localItem = BudgetItem(
        id: itemCreate.id ?? const Uuid().v4(),
        budgetId: budgetId,
        category: itemCreate.category,
        accountNumber: itemCreate.accountNumber,
        budgetedAmount: itemCreate.budgetedAmount,
        budgetType: itemCreate.budgetType,
        createdAt: DateTime.now(),
        updatedAt: DateTime.now(),
      );
      await _dbHelper.insertBudgetItem(budgetId, localItem, isSynced: false);
      print('Offline: Stored budget item locally: ${localItem.category}');
      return localItem;
    }

    try {
      final headers = await _getHeaders();
      final response = await _client.post(
        Uri.parse('$_financeServiceUrl/$budgetId/items/'),
        headers: headers,
        body: json.encode(itemCreate.toJson()),
      );

      if (response.statusCode == 201) {
        print('Online: Successfully created budget item remotely: ${itemCreate.category}');
        return BudgetItem.fromJson(json.decode(response.body));
      } else {
        print('Online: Failed to create budget item remotely: ${response.statusCode} - ${response.body}');
        throw Exception('Failed to create budget item: ${response.body}');
      }
    } catch (e) {
      print('Online: Error creating budget item remotely, attempting local save: $e');
      final localItem = BudgetItem(
        id: itemCreate.id ?? const Uuid().v4(),
        budgetId: budgetId,
        category: itemCreate.category,
        accountNumber: itemCreate.accountNumber,
        budgetedAmount: itemCreate.budgetedAmount,
        budgetType: itemCreate.budgetType,
        createdAt: DateTime.now(),
        updatedAt: DateTime.now(),
      );
      await _dbHelper.insertBudgetItem(budgetId, localItem, isSynced: false);
      throw Exception('Failed to create budget item remotely, saved offline: $e');
    }
  }

  // Future<BudgetItem> updateBudgetItem(String budgetId, String itemId, BudgetItemUpdate item) async {
  //   // For now, update operations require online connection.
  //   final headers = await _getHeaders();
  //   final response = await _client.put(
  //     Uri.parse('$_financeServiceUrl/$budgetId/items/$itemId'),
  //     headers: headers,
  //     body: json.encode(item.toJson()),
  //   );

  //   if (response.statusCode == 200) {
  //     return BudgetItem.fromJson(json.decode(response.body));
  //   } else {
  //     throw Exception('Failed to update budget item: ${response.body}');
  //   }
  // }

  // Future<void> deleteBudgetItem(String budgetId, String itemId) async {
  //   // For now, delete operations require online connection.
  //   final headers = await _getHeaders();
  //   final response = await _client.delete(
  //     Uri.parse('$_financeServiceUrl/$budgetId/items/$itemId'),
  //     headers: headers,
  //   );

  //   if (response.statusCode != 204) {
  //     throw Exception('Failed to delete budget item: ${response.body}');
  //   }
  // }

  // --- Sync Offline Data (NEW) ---
  Future<void> syncOfflineBudgets() async {
    print('Attempting to sync offline budgets...');
    final connectivityResult = await (Connectivity().checkConnectivity());
    if (connectivityResult == ConnectivityResult.none) {
      print('Sync failed: No internet connection.');
      throw Exception('No internet connection to sync data.');
    }

    final unsyncedBudgets = await _dbHelper.getUnsyncedBudgets();
    if (unsyncedBudgets.isEmpty) {
      print('No unsynced budgets found.');
      return;
    }

    print('Found ${unsyncedBudgets.length} unsynced budgets. Starting sync...');
    for (var budget in unsyncedBudgets) {
      try {
        // Create the budget remotely
        final remoteBudget = await createBudget(
          BudgetCreate(
            name: budget.name,
            startDate: budget.startDate,
            endDate: budget.endDate,
            currency: budget.currency,
            description: budget.description,
          )
        );
        // Mark the local budget as synced
        await _dbHelper.markBudgetAsSynced(budget.id!);
        print('Successfully synced offline budget: ${budget.name}');

        // Then sync its items. Assumes remoteBudget.id is the new canonical ID
        for (var item in budget.items) {
          await createBudgetItem(remoteBudget.id!, BudgetItemCreate(
            category: item.category,
            accountNumber: item.accountNumber,
            budgetedAmount: item.budgetedAmount,
            budgetType: item.budgetType,
          ));
          // No need to mark budget item as synced separately, as they are part of the budget's sync lifecycle
        }
      } on http.ClientException catch (e) {
        print('Network error during sync for budget ${budget.id}: $e');
      } catch (e) {
        print('Failed to sync budget ${budget.id}: $e');
      }
    }
    print('Offline budget sync complete.');
  }


  // --- Variance Analysis API Calls ---
  Future<BudgetVarianceReport> getBudgetVarianceReport(String budgetId) async {
    // For now, variance reports require online connection as they aggregate live data.
    final headers = await _getHeaders();
    final response = await _client.get(Uri.parse('$_financeServiceUrl/$budgetId/variance-report'), headers: headers);

    if (response.statusCode == 200) {
      return BudgetVarianceReport.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load budget variance report: ${response.body}');
    }
  }

  // --- Financial Ratios API Calls ---
  Future<FinancialRatiosReport> getFinancialRatios(DateTime startDate, DateTime endDate) async {
    // For now, financial ratios require online connection as they aggregate live data.
    final headers = await _getHeaders();
    final response = await _client.get(
      Uri.parse('$_financialRatiosUrl?start_date=${startDate.toIso8601String()}&end_date=${endDate.toIso8601String()}'),
      headers: headers,
    );

    if (response.statusCode == 200) {
      return FinancialRatiosReport.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to load financial ratios report: ${response.body}');
    }
  }
}
