// ... (Existing Account, JournalLine, JournalEntry, LedgerAccountBalance, TrialBalanceEntry, TrialBalance models) ...

// --- Financial Statement Models (NEW ADDITIONS) ---

// Income Statement
class IncomeStatementItem {
  final String category;
  final double amount;

  IncomeStatementItem({required this.category, required this.amount});

  factory IncomeStatementItem.fromJson(Map<String, dynamic> json) {
    return IncomeStatementItem(
      category: json['category'],
      amount: (json['amount'] as num).toDouble(),
    );
  }
}

class IncomeStatement {
  final DateTime reportDate;
  final DateTime startDate;
  final DateTime endDate;
  final List<IncomeStatementItem> revenues;
  final List<IncomeStatementItem> expenses;
  final double netIncome;

  IncomeStatement({
    required this.reportDate,
    required this.startDate,
    required this.endDate,
    required this.revenues,
    required this.expenses,
    required this.netIncome,
  });

  factory IncomeStatement.fromJson(Map<String, dynamic> json) {
    var revenuesFromJson = json['revenues'] as List;
    List<IncomeStatementItem> revenuesList = revenuesFromJson.map((itemJson) => IncomeStatementItem.fromJson(itemJson)).toList();

    var expensesFromJson = json['expenses'] as List;
    List<IncomeStatementItem> expensesList = expensesFromJson.map((itemJson) => IncomeStatementItem.fromJson(itemJson)).toList();

    return IncomeStatement(
      reportDate: DateTime.parse(json['report_date']),
      startDate: DateTime.parse(json['start_date']),
      endDate: DateTime.parse(json['end_date']),
      revenues: revenuesList,
      expenses: expensesList,
      netIncome: (json['net_income'] as num).toDouble(),
    );
  }
}

// Balance Sheet
class BalanceSheetItem {
  final String category;
  final double amount;

  BalanceSheetItem({required this.category, required this.amount});

  factory BalanceSheetItem.fromJson(Map<String, dynamic> json) {
    return BalanceSheetItem(
      category: json['category'],
      amount: (json['amount'] as num).toDouble(),
    );
  }
}

class BalanceSheet {
  final DateTime reportDate;
  final DateTime asOfDate;
  final List<BalanceSheetItem> assets;
  final List<BalanceSheetItem> liabilities;
  final List<BalanceSheetItem> equity;
  final double totalAssets;
  final double totalLiabilitiesEquity;

  BalanceSheet({
    required this.reportDate,
    required this.asOfDate,
    required this.assets,
    required this.liabilities,
    required this.equity,
    required this.totalAssets,
    required this.totalLiabilitiesEquity,
  });

  factory BalanceSheet.fromJson(Map<String, dynamic> json) {
    var assetsFromJson = json['assets'] as List;
    List<BalanceSheetItem> assetsList = assetsFromJson.map((itemJson) => BalanceSheetItem.fromJson(itemJson)).toList();

    var liabilitiesFromJson = json['liabilities'] as List;
    List<BalanceSheetItem> liabilitiesList = liabilitiesFromJson.map((itemJson) => BalanceSheetItem.fromJson(itemJson)).toList();

    var equityFromJson = json['equity'] as List;
    List<BalanceSheetItem> equityList = equityFromJson.map((itemJson) => BalanceSheetItem.fromJson(itemJson)).toList();

    return BalanceSheet(
      reportDate: DateTime.parse(json['report_date']),
      asOfDate: DateTime.parse(json['as_of_date']),
      assets: assetsList,
      liabilities: liabilitiesList,
      equity: equityList,
      totalAssets: (json['total_assets'] as num).toDouble(),
      totalLiabilitiesEquity: (json['total_liabilities_equity'] as num).toDouble(),
    );
  }
}

// Cash Flow Statement
class CashFlowActivity {
  final String description;
  final double amount;

  CashFlowActivity({required this.description, required this.amount});

  factory CashFlowActivity.fromJson(Map<String, dynamic> json) {
    return CashFlowActivity(
      description: json['description'],
      amount: (json['amount'] as num).toDouble(),
    );
  }
}

class CashFlowSection {
  final String title;
  final List<CashFlowActivity> activities;
  final double netCash;

  CashFlowSection({required this.title, required this.activities, required this.netCash});

  factory CashFlowSection.fromJson(Map<String, dynamic> json) {
    var activitiesFromJson = json['activities'] as List;
    List<CashFlowActivity> activitiesList = activitiesFromJson.map((itemJson) => CashFlowActivity.fromJson(itemJson)).toList();

    return CashFlowSection(
      title: json['title'],
      activities: activitiesList,
      netCash: (json['net_cash'] as num).toDouble(),
    );
  }
}

class CashFlowStatement {
  final DateTime reportDate;
  final DateTime startDate;
  final DateTime endDate;
  final double netIncome;
  final CashFlowSection operatingActivities;
  final CashFlowSection investingActivities;
  final CashFlowSection financingActivities;
  final double netIncreaseDecreaseInCash;
  final double beginningCashBalance;
  final double endingCashBalance;

  CashFlowStatement({
    required this.reportDate,
    required this.startDate,
    required this.endDate,
    required this.netIncome,
    required this.operatingActivities,
    required this.investingActivities,
    required this.financingActivities,
    required this.netIncreaseDecreaseInCash,
    required this.beginningCashBalance,
    required this.endingCashBalance,
  });

  factory CashFlowStatement.fromJson(Map<String, dynamic> json) {
    return CashFlowStatement(
      reportDate: DateTime.parse(json['report_date']),
      startDate: DateTime.parse(json['start_date']),
      endDate: DateTime.parse(json['end_date']),
      netIncome: (json['net_income'] as num).toDouble(),
      operatingActivities: CashFlowSection.fromJson(json['operating_activities']),
      investingActivities: CashFlowSection.fromJson(json['investing_activities']),
      financingActivities: CashFlowSection.fromJson(json['financing_activities']),
      netIncreaseDecreaseInCash: (json['net_increase_decrease_in_cash'] as num).toDouble(),
      beginningCashBalance: (json['beginning_cash_balance'] as num).toDouble(),
      endingCashBalance: (json['ending_cash_balance'] as num).toDouble(),
    );
  }
}
