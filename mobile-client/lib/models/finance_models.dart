// ... (existing BudgetItem, Budget, ActualsSummary, BudgetVarianceItem, BudgetVarianceReport models) ...

// --- Financial Ratio Models (NEW ADDITIONS) ---

class LiquidityRatios {
  final double? currentRatio;
  final double? quickRatio;

  LiquidityRatios({this.currentRatio, this.quickRatio});

  factory LiquidityRatios.fromJson(Map<String, dynamic> json) {
    return LiquidityRatios(
      currentRatio: (json['current_ratio'] as num?)?.toDouble(),
      quickRatio: (json['quick_ratio'] as num?)?.toDouble(),
    );
  }
}

class SolvencyRatios {
  final double? debtToEquityRatio;
  final double? debtToAssetRatio;

  SolvencyRatios({this.debtToEquityRatio, this.debtToAssetRatio});

  factory SolvencyRatios.fromJson(Map<String, dynamic> json) {
    return SolvencyRatios(
      debtToEquityRatio: (json['debt_to_equity_ratio'] as num?)?.toDouble(),
      debtToAssetRatio: (json['debt_to_asset_ratio'] as num?)?.toDouble(),
    );
  }
}

class ProfitabilityRatios {
  final double? grossProfitMargin;
  final double? netProfitMargin;
  final double? returnOnAssets;

  ProfitabilityRatios({this.grossProfitMargin, this.netProfitMargin, this.returnOnAssets});

  factory ProfitabilityRatios.fromJson(Map<String, dynamic> json) {
    return ProfitabilityRatios(
      grossProfitMargin: (json['gross_profit_margin'] as num?)?.toDouble(),
      netProfitMargin: (json['net_profit_margin'] as num?)?.toDouble(),
      returnOnAssets: (json['return_on_assets'] as num?)?.toDouble(),
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
  final String currency;

  FinancialRatiosReport({
    required this.reportDate,
    required this.startDate,
    required this.endDate,
    required this.liquidity,
    required this.solvency,
    required this.profitability,
    required this.currency,
  });

  factory FinancialRatiosReport.fromJson(Map<String, dynamic> json) {
    return FinancialRatiosReport(
      reportDate: DateTime.parse(json['report_date']),
      startDate: DateTime.parse(json['start_date']),
      endDate: DateTime.parse(json['end_date']),
      liquidity: LiquidityRatios.fromJson(json['liquidity']),
      solvency: SolvencyRatios.fromJson(json['solvency']),
      profitability: ProfitabilityRatios.fromJson(json['profitability']),
      currency: json['currency'],
    );
  }
}
