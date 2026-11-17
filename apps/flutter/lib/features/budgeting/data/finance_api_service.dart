import 'dart:convert';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/core/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/features/budgeting/models/finance_models.dart';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:vimbai_mobile_client/core/local_db/database_helper.dart';
import 'package:uuid/uuid.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';

/// Finance service client.
///
/// Budgets are offline-first and live in the local SQLite store: Vimbai has
/// no remote budget CRUD store (the /budget family only exposes calculators
/// like prepare/monitor/variance-analysis). Remote endpoints that DO exist
/// are wired here:
///   POST /budget/variance-analysis   (detailed variance vs actuals)
///   POST /ratio-analysis/ratios      (financial ratios from live statements)
class FinanceApiService {
  final ApiClient _client = ApiClient();
  final String _varianceUrl = '${AppConfig.apiUrl}/budgeting';
  final String _ratiosUrl = '${AppConfig.apiUrl}/calc';

  final DatabaseHelper _dbHelper = DatabaseHelper();
  final AccountingApiService _accounting = AccountingApiService();

  String get _companyId => BookContext.instance.current?.id ?? 'default';

  Future<Map<String, String>> _getHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer $token',
      ...BookContext.instance.headers(),
    };
  }

  // --- Budget CRUD (local store; offline-first by design) ---
  /// Saves a budget with its lines.
  ///
  /// A budget with no lines is an empty shell: the form collects a figure for
  /// each category, so the lines travel in the same call and are written
  /// before the caller is answered. Both the header and every line go through
  /// one path, offline or not - the local store is the source of truth for
  /// budgets (the /budget family only exposes calculators), and [isOffline]
  /// only says whether a replay will come back for these rows.
  Future<Budget> createBudget(
    BudgetCreate budgetCreate, {
    List<BudgetItemCreate> items = const [],
    bool isOffline = false,
  }) async {
    final connectivity = await (Connectivity().checkConnectivity());
    final isOfflineNow =
        connectivity.isEmpty || connectivity.contains(ConnectivityResult.none);
    final offline = isOffline || isOfflineNow;
    final now = DateTime.now();
    final budgetId = budgetCreate.id ?? const Uuid().v4();

    final budget = Budget(
      id: budgetId,
      name: budgetCreate.name,
      startDate: budgetCreate.startDate,
      endDate: budgetCreate.endDate,
      currency: budgetCreate.currency,
      description: budgetCreate.description,
      // Every line is named by the budget it belongs to and given an id here,
      // once, so a line can never be written under one budget and read back
      // under another.
      items: [
        for (final item in items)
          BudgetItem(
            id: item.id ?? const Uuid().v4(),
            budgetId: budgetId,
            category: item.category,
            accountNumber: item.accountNumber,
            budgetedAmount: item.budgetedAmount,
            budgetType: item.budgetType,
            createdAt: now,
            updatedAt: now,
          ),
      ],
      createdAt: now,
      updatedAt: now,
    );

    await _dbHelper.insertBudget(budget, isSynced: !offline);
    print(
      'Stored budget locally: ${budget.name} (${budget.items.length} lines)',
    );
    return budget;
  }

  Future<List<Budget>> getBudgets({bool forceRemote = false}) async {
    print('Fetching budgets from local DB.');
    return await _dbHelper.getBudgetsFromLocal();
  }

  Future<Budget> getBudgetById(String budgetId) async {
    final localBudget = await _dbHelper.getBudget(budgetId);
    if (localBudget != null) {
      return localBudget;
    }
    throw Exception('Budget $budgetId not found.');
  }

  // --- Budget Item CRUD (local store) ---
  Future<BudgetItem> createBudgetItem(
    String budgetId,
    BudgetItemCreate itemCreate, {
    bool isOffline = false,
  }) async {
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
    await _dbHelper.insertBudgetItem(budgetId, localItem, isSynced: !isOffline);
    print('Stored budget item locally: ${localItem.category}');
    return localItem;
  }

  // --- Remote compute: budget variance analysis ---
  /// Detailed variance analysis for a locally stored budget, computed by
  /// the budget-service. [actuals] maps item id -> actual amount; items
  /// without an entry contribute an actual of 0.
  Future<VarianceAnalysisResult> getBudgetVarianceAnalysis(
    String budgetId, {
    Map<String, ExactAmount> actuals = const {},
    double tolerancePercentage = 5.0,
  }) async {
    final budget = await _dbHelper.getBudget(budgetId);
    if (budget == null) {
      throw Exception('Budget $budgetId not found.');
    }
    final items = await _dbHelper.getBudgetItemsForBudget(budgetId);

    final headers = await _getHeaders();
    final response = await _client.post(
      Uri.parse('$_varianceUrl/variance-analysis'),
      headers: headers,
      body: json.encode({
        'company_id': _companyId,
        'budget_id': budgetId,
        'actual_results': items
            .map(
              (item) => {
                'account_id': item.accountNumber,
                'description': item.category,
                'budget': item.budgetedAmount.toDecimalString(),
                'actual': (actuals[item.id] ?? ExactAmount.zero)
                    .toDecimalString(),
              },
            )
            .toList(),
        'tolerance_percentage': tolerancePercentage,
      }),
    );

    if (response.statusCode == 200) {
      return VarianceAnalysisResult.fromJson(
        budgetId: budgetId,
        budgetName: budget.name,
        json: json.decode(response.body) as Map<String, dynamic>,
      );
    } else {
      throw Exception('Failed to run variance analysis: ${response.body}');
    }
  }

  // --- Remote compute: financial ratios ---
  /// Financial ratios for the selected period, computed by the
  /// ratio-analysis-service from the caller's live balance sheet and
  /// income statement (fetched through the accounting service).
  Future<FinancialRatiosReport> getFinancialRatios(
    DateTime startDate,
    DateTime endDate,
  ) async {
    final balanceSheet = await _accounting.getBalanceSheet(endDate);
    final incomeStatement = await _accounting.getIncomeStatement(
      startDate,
      endDate,
    );

    // Summed exactly first, and only then turned into the floating-point figure the ratio engine counts in.
    double sumWhere(List<BalanceSheetItem> items, List<String> keywords) {
      final c = items.where(
        (i) => keywords.any((k) => i.category.toLowerCase().contains(k)),
      );
      return ExactAmount.sum(c.map((i) => i.amount)).toDoubleLossy();
    }

    double sumAll(List<IncomeStatementItem> items) =>
        ExactAmount.sum(items.map((i) => i.amount)).toDoubleLossy();

    final revenue = sumAll(incomeStatement.revenues);
    final cogs = incomeStatement.expenses
        .where(
          (e) =>
              e.category.toLowerCase().contains('cost of goods') ||
              e.category.toLowerCase().contains('cogs'),
        )
        .map((e) => e.amount)
        .fold(ExactAmount.zero, (sum, e) => sum + e)
        .toDoubleLossy();

    final headers = await _getHeaders();
    final response = await _client.post(
      Uri.parse('$_ratiosUrl/ratio-analysis-ratios'),
      headers: headers,
      body: json.encode({
        'company_id': _companyId,
        'period': '${endDate.year}-${endDate.month.toString().padLeft(2, '0')}',
        'balance_sheet': {
          'current_assets': sumWhere(balanceSheet.assets, const ['current']),
          'current_liabilities': sumWhere(balanceSheet.liabilities, const [
            'current',
          ]),
          'total_assets': balanceSheet.totalAssets.toDoubleLossy(),
          'total_debt': ExactAmount.sum(
            balanceSheet.liabilities.map((i) => i.amount),
          ).toDoubleLossy(),
          'total_equity': _equityTotal(balanceSheet),
          'inventory': sumWhere(balanceSheet.assets, const ['inventory']),
          'cash': sumWhere(balanceSheet.assets, const ['cash']),
          'marketable_securities': sumWhere(balanceSheet.assets, const [
            'marketable',
            'securit',
          ]),
          'accounts_receivable': sumWhere(balanceSheet.assets, const [
            'receivable',
          ]),
          'accounts_payable': sumWhere(balanceSheet.liabilities, const [
            'payable',
          ]),
        },
        'income_statement': {
          'revenue': revenue,
          'net_income': incomeStatement.netIncome.toDoubleLossy(),
          'cogs': cogs,
          'interest_expense': incomeStatement.expenses
              .where((e) => e.category.toLowerCase().contains('interest'))
              .map((e) => e.amount)
              .fold(ExactAmount.zero, (sum, e) => sum + e)
              .toDoubleLossy(),
        },
        'cash_flow': <String, double>{},
      }),
    );

    if (response.statusCode == 200) {
      return _mapRatiosResponse(
        json.decode(response.body) as Map<String, dynamic>,
        startDate,
        endDate,
      );
    } else {
      throw Exception(
        'Failed to load financial ratios report: ${response.body}',
      );
    }
  }

  static double _equityTotal(BalanceSheet bs) =>
      ExactAmount.sum(bs.equity.map((i) => i.amount)).toDoubleLossy();

  FinancialRatiosReport _mapRatiosResponse(
    Map<String, dynamic> j,
    DateTime startDate,
    DateTime endDate,
  ) {
    final liquidity =
        (j['liquidity_ratios'] as Map<String, dynamic>?) ?? const {};
    final leverage =
        (j['leverage_ratios'] as Map<String, dynamic>?) ?? const {};
    final profitability =
        (j['profitability_ratios'] as Map<String, dynamic>?) ?? const {};
    final efficiency =
        (j['efficiency_ratios'] as Map<String, dynamic>?) ?? const {};
    final valuation =
        (j['valuation_ratios'] as Map<String, dynamic>?) ?? const {};

    double? pct(num? v) =>
        v == null ? null : v.toDouble() / 100.0; // backend %, model fraction

    double? d(num? v) => v == null ? null : v.toDouble();

    return FinancialRatiosReport(
      reportDate: DateTime.now(),
      startDate: startDate,
      endDate: endDate,
      liquidity: LiquidityRatios(
        currentRatio: d(liquidity['current_ratio']),
        quickRatio: d(liquidity['quick_ratio']),
        cashRatio: d(liquidity['cash_ratio']),
      ),
      solvency: SolvencyRatios(
        debtToEquityRatio: d(leverage['debt_to_equity']),
        debtToAssetRatio: d(leverage['debt_to_assets']),
        equityMultiplier: d(leverage['equity_multiplier']),
        timesInterestEarned: d(leverage['interest_coverage']),
      ),
      profitability: ProfitabilityRatios(
        grossProfitMargin: pct(profitability['gross_margin']),
        operatingProfitMargin: pct(profitability['operating_margin']),
        netProfitMargin: pct(profitability['net_margin']),
        returnOnAssets: pct(profitability['roa']),
        returnOnEquity: pct(profitability['roe']),
      ),
      efficiency: EfficiencyRatios(
        inventoryTurnover: d(efficiency['inventory_turnover']),
        accountsReceivableTurnover: d(efficiency['ar_turnover']),
        accountsPayableTurnover: d(efficiency['ap_turnover']),
        assetTurnover: d(efficiency['asset_turnover']),
        daySalesOutstanding: d(efficiency['days_sales_outstanding']),
      ),
      marketValue: MarketValueRatios(
        priceToEarningsRatio: d(valuation['pe_ratio']),
      ),
    );
  }

  // --- Sync Offline Data ---
  Future<void> syncOfflineData() async {
    // Budgets and budget items are local-only; nothing to push remotely.
    // Journal-entry sync is handled by the accounting sync flow.
    await _dbHelper.getUnsyncedBudgets(); // kept for future use
  }
}
