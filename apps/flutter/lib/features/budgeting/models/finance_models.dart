import '../../../core/api/amount.dart';

// mobile-client/lib/models/finance_models.dart

// --- Budget Models ---

class BudgetItem {
  final String? id;
  final String? budgetId;
  final String category;
  final String accountNumber;
  final double budgetedAmount;
  final String? budgetType;
  final DateTime createdAt;
  final DateTime updatedAt;

  BudgetItem({
    this.id,
    this.budgetId,
    required this.category,
    required this.accountNumber,
    required this.budgetedAmount,
    this.budgetType,
    required this.createdAt,
    required this.updatedAt,
  });

  factory BudgetItem.fromJson(Map<String, dynamic> json) {
    return BudgetItem(
      id: json['id'],
      budgetId: json['budget_id'],
      category: json['category'] ?? '',
      accountNumber: json['account_number'] ?? '',
      budgetedAmount: amountFromJson(json['budgeted_amount']),
      budgetType: json['budget_type'],
      createdAt: json['created_at'] != null
          ? DateTime.parse(json['created_at'])
          : DateTime.now(),
      updatedAt: json['updated_at'] != null
          ? DateTime.parse(json['updated_at'])
          : DateTime.now(),
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'budget_id': budgetId,
      'category': category,
      'account_number': accountNumber,
      'budgeted_amount': budgetedAmount,
      'budget_type': budgetType,
    };
  }
}

class Budget {
  final String? id;
  final String name;
  final DateTime startDate;
  final DateTime endDate;
  final String currency;
  final String? description;
  final List<BudgetItem> items;
  final bool isSynced;
  final DateTime createdAt;
  final DateTime updatedAt;

  Budget({
    this.id,
    required this.name,
    required this.startDate,
    required this.endDate,
    required this.currency,
    this.description,
    this.items = const [],
    this.isSynced = false,
    required this.createdAt,
    required this.updatedAt,
  });

  factory Budget.fromJson(Map<String, dynamic> json) {
    List<BudgetItem> itemsList = [];
    if (json['items'] != null) {
      itemsList = (json['items'] as List)
          .map((item) => BudgetItem.fromJson(item))
          .toList();
    }

    return Budget(
      id: json['id'],
      name: json['name'] ?? '',
      startDate: json['start_date'] != null
          ? DateTime.parse(json['start_date'])
          : DateTime.now(),
      endDate: json['end_date'] != null
          ? DateTime.parse(json['end_date'])
          : DateTime.now(),
      currency: json['currency'] ?? 'USD',
      description: json['description'],
      items: itemsList,
      isSynced: json['is_synced'] ?? false,
      createdAt: json['created_at'] != null
          ? DateTime.parse(json['created_at'])
          : DateTime.now(),
      updatedAt: json['updated_at'] != null
          ? DateTime.parse(json['updated_at'])
          : DateTime.now(),
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'name': name,
      'start_date': startDate.toIso8601String(),
      'end_date': endDate.toIso8601String(),
      'currency': currency,
      'description': description,
      'items': items.map((item) => item.toJson()).toList(),
      'is_synced': isSynced,
    };
  }
}

class BudgetCreate {
  final String? id;
  final String name;
  final DateTime startDate;
  final DateTime endDate;
  final String currency;
  final String? description;

  BudgetCreate({
    this.id,
    required this.name,
    required this.startDate,
    required this.endDate,
    required this.currency,
    this.description,
  });

  Map<String, dynamic> toJson() {
    return {
      'name': name,
      'start_date': startDate.toIso8601String(),
      'end_date': endDate.toIso8601String(),
      'currency': currency,
      'description': description,
    };
  }
}

class BudgetItemCreate {
  final String? id;
  final String category;
  final String accountNumber;
  final double budgetedAmount;
  final String? budgetType;

  BudgetItemCreate({
    this.id,
    required this.category,
    required this.accountNumber,
    required this.budgetedAmount,
    this.budgetType,
  });

  Map<String, dynamic> toJson() {
    return {
      'category': category,
      'account_number': accountNumber,
      'budgeted_amount': budgetedAmount,
      'budget_type': budgetType,
    };
  }
}

// --- Variance Analysis Models ---

class BudgetVarianceItem {
  final String category;
  final String accountNumber;
  final double budgetedAmount;
  final double actualAmount;
  final double variance;
  final double variancePercentage;

  BudgetVarianceItem({
    required this.category,
    required this.accountNumber,
    required this.budgetedAmount,
    required this.actualAmount,
    required this.variance,
    required this.variancePercentage,
  });

  factory BudgetVarianceItem.fromJson(Map<String, dynamic> json) {
    return BudgetVarianceItem(
      category: json['category'] ?? '',
      accountNumber: json['account_number'] ?? '',
      budgetedAmount: amountFromJson(json['budgeted_amount']),
      actualAmount: amountFromJson(json['actual_amount']),
      variance: amountFromJson(json['variance']),
      variancePercentage: amountFromJson(json['variance_percentage']),
    );
  }
}

class BudgetVarianceReport {
  final String? id;
  final String budgetId;
  final String budgetName;
  final DateTime startDate;
  final DateTime endDate;
  final double totalBudgeted;
  final double totalActual;
  final double totalVariance;
  final List<BudgetVarianceItem> varianceItems;
  final DateTime generatedAt;

  BudgetVarianceReport({
    this.id,
    required this.budgetId,
    required this.budgetName,
    required this.startDate,
    required this.endDate,
    required this.totalBudgeted,
    required this.totalActual,
    required this.totalVariance,
    this.varianceItems = const [],
    required this.generatedAt,
  });

  factory BudgetVarianceReport.fromJson(Map<String, dynamic> json) {
    List<BudgetVarianceItem> itemsList = [];
    if (json['variance_items'] != null) {
      itemsList = (json['variance_items'] as List)
          .map((item) => BudgetVarianceItem.fromJson(item))
          .toList();
    }

    return BudgetVarianceReport(
      id: json['id'],
      budgetId: json['budget_id'] ?? '',
      budgetName: json['budget_name'] ?? '',
      startDate: json['start_date'] != null
          ? DateTime.parse(json['start_date'])
          : DateTime.now(),
      endDate: json['end_date'] != null
          ? DateTime.parse(json['end_date'])
          : DateTime.now(),
      totalBudgeted: amountFromJson(json['total_budgeted']),
      totalActual: amountFromJson(json['total_actual']),
      totalVariance: amountFromJson(json['total_variance']),
      varianceItems: itemsList,
      generatedAt: json['generated_at'] != null
          ? DateTime.parse(json['generated_at'])
          : DateTime.now(),
    );
  }
}

// --- Financial Ratio Models ---

class LiquidityRatios {
  final double? currentRatio;
  final double? quickRatio;
  final double? cashRatio;
  final double? workingCapital;

  LiquidityRatios({
    this.currentRatio,
    this.quickRatio,
    this.cashRatio,
    this.workingCapital,
  });

  factory LiquidityRatios.fromJson(Map<String, dynamic> json) {
    return LiquidityRatios(
      currentRatio: amountFromJson(json['current_ratio']),
      quickRatio: amountFromJson(json['quick_ratio']),
      cashRatio: amountFromJson(json['cash_ratio']),
      workingCapital: amountFromJson(json['working_capital']),
    );
  }
}

class SolvencyRatios {
  final double? debtToEquityRatio;
  final double? debtToAssetRatio;
  final double? equityMultiplier;
  final double? timesInterestEarned;

  SolvencyRatios({
    this.debtToEquityRatio,
    this.debtToAssetRatio,
    this.equityMultiplier,
    this.timesInterestEarned,
  });

  factory SolvencyRatios.fromJson(Map<String, dynamic> json) {
    return SolvencyRatios(
      debtToEquityRatio: amountFromJson(json['debt_to_equity_ratio']),
      debtToAssetRatio: amountFromJson(json['debt_to_asset_ratio']),
      equityMultiplier: amountFromJson(json['equity_multiplier']),
      timesInterestEarned: amountFromJson(json['times_interest_earned']),
    );
  }
}

class ProfitabilityRatios {
  final double? grossProfitMargin;
  final double? operatingProfitMargin;
  final double? netProfitMargin;
  final double? returnOnAssets;
  final double? returnOnEquity;

  ProfitabilityRatios({
    this.grossProfitMargin,
    this.operatingProfitMargin,
    this.netProfitMargin,
    this.returnOnAssets,
    this.returnOnEquity,
  });

  factory ProfitabilityRatios.fromJson(Map<String, dynamic> json) {
    return ProfitabilityRatios(
      grossProfitMargin: amountFromJson(json['gross_profit_margin']),
      operatingProfitMargin: (json['operating_profit_margin'] as num?)
          ?.toDouble(),
      netProfitMargin: amountFromJson(json['net_profit_margin']),
      returnOnAssets: amountFromJson(json['return_on_assets']),
      returnOnEquity: amountFromJson(json['return_on_equity']),
    );
  }
}

class EfficiencyRatios {
  final double? inventoryTurnover;
  final double? accountsReceivableTurnover;
  final double? accountsPayableTurnover;
  final double? assetTurnover;
  final double? daySalesOutstanding;

  EfficiencyRatios({
    this.inventoryTurnover,
    this.accountsReceivableTurnover,
    this.accountsPayableTurnover,
    this.assetTurnover,
    this.daySalesOutstanding,
  });

  factory EfficiencyRatios.fromJson(Map<String, dynamic> json) {
    return EfficiencyRatios(
      inventoryTurnover: amountFromJson(json['inventory_turnover']),
      accountsReceivableTurnover: (json['accounts_receivable_turnover'] as num?)
          ?.toDouble(),
      accountsPayableTurnover: (json['accounts_payable_turnover'] as num?)
          ?.toDouble(),
      assetTurnover: amountFromJson(json['asset_turnover']),
      daySalesOutstanding: amountFromJson(json['day_sales_outstanding']),
    );
  }
}

class MarketValueRatios {
  final double? earningsPerShare;
  final double? priceToEarningsRatio;
  final double? bookValuePerShare;

  MarketValueRatios({
    this.earningsPerShare,
    this.priceToEarningsRatio,
    this.bookValuePerShare,
  });

  factory MarketValueRatios.fromJson(Map<String, dynamic> json) {
    return MarketValueRatios(
      earningsPerShare: amountFromJson(json['earnings_per_share']),
      priceToEarningsRatio: (json['price_to_earnings_ratio'] as num?)
          ?.toDouble(),
      bookValuePerShare: amountFromJson(json['book_value_per_share']),
    );
  }
}

class FinancialRatiosReport {
  final DateTime reportDate;
  final DateTime startDate;
  final DateTime endDate;
  final LiquidityRatios liquidity;
  final SolvencyRatios solvency;
  final ProfitabilityRatios profitability;
  final EfficiencyRatios efficiency;
  final MarketValueRatios marketValue;
  final String currency;

  FinancialRatiosReport({
    required this.reportDate,
    required this.startDate,
    required this.endDate,
    required this.liquidity,
    required this.solvency,
    required this.profitability,
    required this.efficiency,
    required this.marketValue,
    this.currency = 'USD',
  });

  factory FinancialRatiosReport.fromJson(Map<String, dynamic> json) {
    return FinancialRatiosReport(
      reportDate: json['report_date'] != null
          ? DateTime.parse(json['report_date'])
          : DateTime.now(),
      startDate: json['start_date'] != null
          ? DateTime.parse(json['start_date'])
          : DateTime.now(),
      endDate: json['end_date'] != null
          ? DateTime.parse(json['end_date'])
          : DateTime.now(),
      liquidity: json['liquidity'] != null
          ? LiquidityRatios.fromJson(json['liquidity'])
          : LiquidityRatios(),
      solvency: json['solvency'] != null
          ? SolvencyRatios.fromJson(json['solvency'])
          : SolvencyRatios(),
      profitability: json['profitability'] != null
          ? ProfitabilityRatios.fromJson(json['profitability'])
          : ProfitabilityRatios(),
      efficiency: json['efficiency'] != null
          ? EfficiencyRatios.fromJson(json['efficiency'])
          : EfficiencyRatios(),
      marketValue: json['market_value'] != null
          ? MarketValueRatios.fromJson(json['market_value'])
          : MarketValueRatios(),
      currency: json['currency'] ?? 'USD',
    );
  }
}

// --- Budget variance analysis (budget-service POST /variance-analysis) ---
class VarianceOutsideToleranceItem {
  final String accountId;
  final String description;
  final double budget;
  final double actual;
  final double variance;
  final double variancePct;

  VarianceOutsideToleranceItem({
    required this.accountId,
    required this.description,
    required this.budget,
    required this.actual,
    required this.variance,
    required this.variancePct,
  });

  factory VarianceOutsideToleranceItem.fromJson(Map<String, dynamic> json) {
    return VarianceOutsideToleranceItem(
      accountId: (json['account_id'] as String?) ?? '',
      description: (json['description'] as String?) ?? '',
      budget: amountFromJson(json['budget']),
      actual: amountFromJson(json['actual']),
      variance: amountFromJson(json['variance']),
      variancePct: amountFromJson(json['variance_pct']),
    );
  }
}

class VarianceAnalysisResult {
  final String budgetId;
  final String budgetName;
  final double totalVariance;
  final double favorableVariance;
  final double unfavorableVariance;
  final List<VarianceOutsideToleranceItem> itemsOutsideTolerance;
  final Map<String, String> rootCauses;
  final List<String> correctiveActions;

  VarianceAnalysisResult({
    required this.budgetId,
    required this.budgetName,
    required this.totalVariance,
    required this.favorableVariance,
    required this.unfavorableVariance,
    required this.itemsOutsideTolerance,
    required this.rootCauses,
    required this.correctiveActions,
  });

  factory VarianceAnalysisResult.fromJson({
    required String budgetId,
    required String budgetName,
    required Map<String, dynamic> json,
  }) {
    final outside = (json['items_outside_tolerance'] as List<dynamic>? ?? [])
        .map(
          (e) =>
              VarianceOutsideToleranceItem.fromJson(e as Map<String, dynamic>),
        )
        .toList();
    final rawCauses =
        (json['root_causes'] as Map<String, dynamic>? ?? const {});
    final causes = <String, String>{};
    rawCauses.forEach((k, v) => causes[k] = v.toString());

    return VarianceAnalysisResult(
      budgetId: (json['budget_id'] as String?) ?? budgetId,
      budgetName: budgetName,
      totalVariance: amountFromJson(json['total_variance']),
      favorableVariance: amountFromJson(json['favorable_variance']),
      unfavorableVariance: amountFromJson(json['unfavorable_variance']),
      itemsOutsideTolerance: outside,
      rootCauses: causes,
      correctiveActions:
          (json['corrective_actions'] as List<dynamic>? ?? const [])
              .map((e) => e.toString())
              .toList(),
    );
  }
}
