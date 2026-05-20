// ... (existing BudgetItem and Budget models) ...

// --- Variance Analysis Models (NEW ADDITIONS) ---

class ActualsSummary {
  final String accountNumber;
  final double totalDebit;
  final double totalCredit;
  final double balance;

  ActualsSummary({
    required this.accountNumber,
    required this.totalDebit,
    required this.totalCredit,
    required this.balance,
  });

  factory ActualsSummary.fromJson(Map<String, dynamic> json) {
    return ActualsSummary(
      accountNumber: json['account_number'],
      totalDebit: (json['total_debit'] as num).toDouble(),
      totalCredit: (json['total_credit'] as num).toDouble(),
      balance: (json['balance'] as num).toDouble(),
    );
  }
}

class BudgetVarianceItem {
  final String category;
  final String? accountNumber;
  final double budgetedAmount;
  final double actualAmount;
  final double variance;
  final double variancePercentage;

  BudgetVarianceItem({
    required this.category,
    this.accountNumber,
    required this.budgetedAmount,
    required this.actualAmount,
    required this.variance,
    required this.variancePercentage,
  });

  factory BudgetVarianceItem.fromJson(Map<String, dynamic> json) {
    return BudgetVarianceItem(
      category: json['category'],
      accountNumber: json['account_number'],
      budgetedAmount: (json['budgeted_amount'] as num).toDouble(),
      actualAmount: (json['actual_amount'] as num).toDouble(),
      variance: (json['variance'] as num).toDouble(),
      variancePercentage: (json['variance_percentage'] as num).toDouble(),
    );
  }
}

class BudgetVarianceReport {
  final String budgetName;
  final int fiscalYear;
  final String period;
  final DateTime reportDate;
  final List<BudgetVarianceItem> items;
  final double totalBudgeted;
  final double totalActual;
  final double totalVariance;
  final double totalVariancePercentage;

  BudgetVarianceReport({
    required this.budgetName,
    required this.fiscalYear,
    required this.period,
    required this.reportDate,
    required this.items,
    required this.totalBudgeted,
    required this.totalActual,
    required this.totalVariance,
    required this.totalVariancePercentage,
  });

  factory BudgetVarianceReport.fromJson(Map<String, dynamic> json) {
    var itemsFromJson = json['items'] as List;
    List<BudgetVarianceItem> itemsList = itemsFromJson.map((itemJson) => BudgetVarianceItem.fromJson(itemJson)).toList();

    return BudgetVarianceReport(
      budgetName: json['budget_name'],
      fiscalYear: json['fiscal_year'],
      period: json['period'],
      reportDate: DateTime.parse(json['report_date']),
      items: itemsList,
      totalBudgeted: (json['total_budgeted'] as num).toDouble(),
      totalActual: (json['total_actual'] as num).toDouble(),
      totalVariance: (json['variance'] as num).toDouble(),
      totalVariancePercentage: (json['total_variance_percentage'] as num).toDouble(),
    );
  }
}
