import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:finacc_mobile_client/local_db/user_local_data.dart';
import 'package:finacc_mobile_client/config.dart'; // For API URL

// Models needed for Chart of Accounts and Journal Entries
// (Assuming these models are defined in mobile-client/lib/models/accounting_models.dart,
// or are simple maps/dynamic types for now)
class Account {
  final String accountNumber;
  final String accountName;
  final String accountType;
  final String normalBalance;
  final String? description;
  final String? parentAccountNumber;
  final String id;
  final DateTime createdAt;
  final DateTime updatedAt;

  Account({
    required this.accountNumber,
    required this.accountName,
    required this.accountType,
    required this.normalBalance,
    this.description,
    this.parentAccountNumber,
    required this.id,
    required this.createdAt,
    required this.updatedAt,
  });

  factory Account.fromJson(Map<String, dynamic> json) {
    return Account(
      accountNumber: json['account_number'],
      accountName: json['account_name'],
      accountType: json['account_type'],
      normalBalance: json['normal_balance'],
      description: json['description'],
      parentAccountNumber: json['parent_account_number'],
      id: json['id'],
      createdAt: DateTime.parse(json['created_at']),
      updatedAt: DateTime.parse(json['updated_at']),
    );
  }
}

// For Journal Entries
class JournalLine {
  final String accountNumber;
  final double debit;
  final double credit;
  final String? description;

  JournalLine({
    required this.accountNumber,
    required this.debit,
    required this.credit,
    this.description,
  });

  Map<String, dynamic> toJson() {
    return {
      'account_number': accountNumber,
      'debit': debit,
      'credit': credit,
      'description': description,
    };
  }
}

class JournalEntry {
  final DateTime entryDate;
  final String description;
  final String? referenceNumber;
  final String sourceModule;
  final List<JournalLine> lines;
  final String? id; // Optional for creation, required for fetched entry
  final DateTime? createdAt;
  final DateTime? updatedAt;

  JournalEntry({
    required this.entryDate,
    required this.description,
    this.referenceNumber,
    required this.sourceModule,
    required this.lines,
    this.id,
    this.createdAt,
    this.updatedAt,
  });

  factory JournalEntry.fromJson(Map<String, dynamic> json) {
    var linesFromJson = json['lines'] as List;
    List<JournalLine> linesList = linesFromJson.map((lineJson) => JournalLine(
      accountNumber: lineJson['account_number'],
      debit: (lineJson['debit'] as num).toDouble(),
      credit: (lineJson['credit'] as num).toDouble(),
      description: lineJson['description'],
    )).toList();

    return JournalEntry(
      entryDate: DateTime.parse(json['entry_date']),
      description: json['description'],
      referenceNumber: json['reference_number'],
      sourceModule: json['source_module'],
      lines: linesList,
      id: json['id'],
      createdAt: json['created_at'] != null ? DateTime.parse(json['created_at']) : null,
      updatedAt: json['updated_at'] != null ? DateTime.parse(json['updated_at']) : null,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'entry_date': entryDate.toIso8601String(),
      'description': description,
      'reference_number': referenceNumber,
      'source_module': sourceModule,
      'lines': lines.map((line) => line.toJson()).toList(),
    };
  }
}

// For Ledger Balances
class LedgerAccountBalance {
  final String accountNumber;
  final String accountName;
  final String accountType;
  final String normalBalance;
  final double currentBalance;

  LedgerAccountBalance({
    required this.accountNumber,
    required this.accountName,
    required this.accountType,
    required this.normalBalance,
    required this.currentBalance,
  });

  factory LedgerAccountBalance.fromJson(Map<String, dynamic> json) {
    return LedgerAccountBalance(
      accountNumber: json['account_number'],
      accountName: json['account_name'],
      accountType: json['account_type'],
      normalBalance: json['normal_balance'],
      currentBalance: (json['current_balance'] as num).toDouble(),
    );
  }
}

// For Trial Balance
class TrialBalanceEntry {
  final String accountNumber;
  final String accountName;
  final double debitTotal;
  final double creditTotal;

  TrialBalanceEntry({
    required this.accountNumber,
    required this.accountName,
    required this.debitTotal,
    required this.creditTotal,
  });

  factory TrialBalanceEntry.fromJson(Map<String, dynamic> json) {
    return TrialBalanceEntry(
      accountNumber: json['account_number'],
      accountName: json['account_name'],
      debitTotal: (json['debit_total'] as num).toDouble(),
      creditTotal: (json['credit_total'] as num).toDouble(),
    );
  }
}

class TrialBalance {
  final DateTime reportDate;
  final List<TrialBalanceEntry> entries;
  final double totalDebits;
  final double totalCredits;

  TrialBalance({
    required this.reportDate,
    required this.entries,
    required this.totalDebits,
    required this.totalCredits,
  });

  factory TrialBalance.fromJson(Map<String, dynamic> json) {
    var entriesFromJson = json['entries'] as List;
    List<TrialBalanceEntry> entriesList = entriesFromJson.map((entryJson) => TrialBalanceEntry.fromJson(entryJson)).toList();

    return TrialBalance(
      reportDate: DateTime.parse(json['report_date']),
      entries: entriesList,
      totalDebits: (json['total_debits'] as num).toDouble(),
      totalCredits: (json['total_credits'] as num).toDouble(),
    );
  }
}

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
