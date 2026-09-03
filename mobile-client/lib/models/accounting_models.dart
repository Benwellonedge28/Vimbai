// mobile-client/lib/models/accounting_models.dart

import 'package:json_annotation/json_annotation.dart';

part 'accounting_models.g.dart'; // Generated file for json_serializable

// --- Journal Entry Models ---
@JsonSerializable(explicitToJson: true)
class JournalLineCreate {
  final String accountNumber;
  final double debit;
  final double credit;
  final String? description;

  JournalLineCreate({
    required this.accountNumber,
    required this.debit,
    required this.credit,
    this.description,
  });

  factory JournalLineCreate.fromJson(Map<String, dynamic> json) => _$JournalLineCreateFromJson(json);
  Map<String, dynamic> toJson() => _$JournalLineCreateToJson(this);
}

@JsonSerializable(explicitToJson: true)
class JournalEntryCreate {
  final DateTime entryDate;
  final String description;
  final String sourceModule;
  final String? referenceNumber;
  final List<JournalLineCreate> lines;

  JournalEntryCreate({
    required this.entryDate,
    required this.description,
    required this.sourceModule,
    this.referenceNumber,
    required this.lines,
  });

  factory JournalEntryCreate.fromJson(Map<String, dynamic> json) => _$JournalEntryCreateFromJson(json);
  Map<String, dynamic> toJson() => _$JournalEntryCreateToJson(this);
}

@JsonSerializable(explicitToJson: true)
class JournalEntryUpdate {
  final DateTime? entryDate;
  final String? description;
  final String? sourceModule;
  final String? referenceNumber;
  final List<JournalLineCreate>? lines;

  JournalEntryUpdate({
    this.entryDate,
    this.description,
    this.sourceModule,
    this.referenceNumber,
    this.lines,
  });

  factory JournalEntryUpdate.fromJson(Map<String, dynamic> json) => _$JournalEntryUpdateFromJson(json);
  Map<String, dynamic> toJson() => _$JournalEntryUpdateToJson(this);
}

@JsonSerializable(explicitToJson: true)
class JournalEntryInDB {
  final String id;
  final DateTime entryDate;
  final String description;
  final String sourceModule;
  final String? referenceNumber;
  final List<JournalLineInDB> lines; // Note: using JournalLineInDB for fetched data
  final DateTime createdAt;
  final DateTime updatedAt;

  JournalEntryInDB({
    required this.id,
    required this.entryDate,
    required this.description,
    required this.sourceModule,
    this.referenceNumber,
    required this.lines,
    required this.createdAt,
    required this.updatedAt,
  });

  factory JournalEntryInDB.fromJson(Map<String, dynamic> json) => _$JournalEntryInDBFromJson(json);
  Map<String, dynamic> toJson() => _$JournalEntryInDBToJson(this);
}

// Separate model for lines in DB, as it might have its own ID or timestamp
@JsonSerializable(explicitToJson: true)
class JournalLineInDB {
  final String id;
  final String accountNumber;
  final double debit;
  final double credit;
  final String? description;

  JournalLineInDB({
    required this.id,
    required this.accountNumber,
    required this.debit,
    required this.credit,
    this.description,
  });

  factory JournalLineInDB.fromJson(Map<String, dynamic> json) => _$JournalLineInDBFromJson(json);
  Map<String, dynamic> toJson() => _$JournalLineInDBToJson(this);
}


// --- Account Models ---
@JsonSerializable(explicitToJson: true)
class AccountCreate {
  final String accountNumber;
  final String accountName;
  final String accountType;
  final double currentBalance;
  final String? description;

  AccountCreate({
    required this.accountNumber,
    required this.accountName,
    required this.accountType,
    required this.currentBalance,
    this.description,
  });

  factory AccountCreate.fromJson(Map<String, dynamic> json) => _$AccountCreateFromJson(json);
  Map<String, dynamic> toJson() => _$AccountCreateToJson(this);
}

@JsonSerializable(explicitToJson: true)
class AccountUpdate {
  final String? accountName;
  final String? accountType;
  final double? currentBalance;
  final String? description;

  AccountUpdate({
    this.accountName,
    this.accountType,
    this.currentBalance,
    this.description,
  });

  factory AccountUpdate.fromJson(Map<String, dynamic> json) => _$AccountUpdateFromJson(json);
  Map<String, dynamic> toJson() => _$AccountUpdateToJson(this);
}

@JsonSerializable(explicitToJson: true)
class AccountInDB {
  final String id;
  final String accountNumber;
  final String accountName;
  final String accountType;
  final double currentBalance;
  final String? description;
  final DateTime createdAt;
  final DateTime updatedAt;

  AccountInDB({
    required this.id,
    required this.accountNumber,
    required this.accountName,
    required this.accountType,
    required this.currentBalance,
    this.description,
    required this.createdAt,
    required this.updatedAt,
  });

  factory AccountInDB.fromJson(Map<String, dynamic> json) => _$AccountInDBFromJson(json);
  Map<String, dynamic> toJson() => _$AccountInDBToJson(this);
}

// --- Placeholder for Auth Service ---
// This would be a real service for handling tokens, login, etc.
// --- Mobile-facing Journal Entry models (offline-first) ---
@JsonSerializable(explicitToJson: true)
class JournalLine {
  final String accountNumber;
  final String? description;
  final double debit;
  final double credit;

  JournalLine({
    required this.accountNumber,
    this.description,
    this.debit = 0.0,
    this.credit = 0.0,
  });

  factory JournalLine.fromJson(Map<String, dynamic> json) => _$JournalLineFromJson(json);
  Map<String, dynamic> toJson() => _$JournalLineToJson(this);
}

@JsonSerializable(explicitToJson: true)
class JournalEntry {
  final String id;
  @JsonKey(name: 'entry_date')
  final DateTime entryDate;
  final String description;
  @JsonKey(name: 'reference_number')
  final String? referenceNumber;
  @JsonKey(name: 'source_module')
  final String sourceModule;
  final String? status;
  final List<JournalLine> lines;
  @JsonKey(name: 'created_at')
  final DateTime? createdAt;
  @JsonKey(name: 'updated_at')
  final DateTime? updatedAt;
  @JsonKey(name: 'is_synced', includeIfNull: false)
  final bool? isSynced;

  JournalEntry({
    required this.id,
    required this.entryDate,
    required this.description,
    this.referenceNumber,
    required this.sourceModule,
    this.status,
    required this.lines,
    this.createdAt,
    this.updatedAt,
    this.isSynced,
  });

  factory JournalEntry.fromJson(Map<String, dynamic> json) => _$JournalEntryFromJson(json);
  Map<String, dynamic> toJson() => _$JournalEntryToJson(this);
}

// --- Chart of Accounts (mobile view model) ---
@JsonSerializable()
class Account {
  final String? id;
  @JsonKey(name: 'account_number')
  final String accountNumber;
  @JsonKey(name: 'name')
  final String accountName;
  @JsonKey(name: 'account_type')
  final String accountType;
  @JsonKey(name: 'normal_balance')
  final String normalBalance;
  final String? description;
  @JsonKey(name: 'parent_account_number')
  final String? parentAccountNumber;
  @JsonKey(name: 'created_at')
  final DateTime? createdAt;
  @JsonKey(name: 'updated_at')
  final DateTime? updatedAt;

  Account({
    this.id,
    required this.accountNumber,
    required this.accountName,
    required this.accountType,
    required this.normalBalance,
    this.description,
    this.parentAccountNumber,
    this.createdAt,
    this.updatedAt,
  });

  factory Account.fromJson(Map<String, dynamic> json) => _$AccountFromJson(json);
  Map<String, dynamic> toJson() => _$AccountToJson(this);
}

// --- Trial Balance (maps backend TrialBalanceReport) ---
@JsonSerializable(explicitToJson: true)
class TrialBalanceEntry {
  @JsonKey(name: 'account_number')
  final String accountNumber;
  @JsonKey(name: 'account_name')
  final String accountName;
  @JsonKey(name: 'account_type')
  final String accountType;
  @JsonKey(name: 'debit')
  final double debitTotal;
  @JsonKey(name: 'credit')
  final double creditTotal;

  TrialBalanceEntry({
    required this.accountNumber,
    required this.accountName,
    required this.accountType,
    this.debitTotal = 0.0,
    this.creditTotal = 0.0,
  });

  factory TrialBalanceEntry.fromJson(Map<String, dynamic> json) => _$TrialBalanceEntryFromJson(json);
  Map<String, dynamic> toJson() => _$TrialBalanceEntryToJson(this);
}

@JsonSerializable(explicitToJson: true)
class TrialBalance {
  @JsonKey(name: 'report_date')
  final DateTime reportDate;
  final List<TrialBalanceEntry> entries;
  @JsonKey(name: 'total_debits')
  final double totalDebits;
  @JsonKey(name: 'total_credits')
  final double totalCredits;
  @JsonKey(name: 'is_balanced')
  final bool isBalanced;

  TrialBalance({
    required this.reportDate,
    required this.entries,
    this.totalDebits = 0.0,
    this.totalCredits = 0.0,
    this.isBalanced = false,
  });

  factory TrialBalance.fromJson(Map<String, dynamic> json) => _$TrialBalanceFromJson(json);
  Map<String, dynamic> toJson() => _$TrialBalanceToJson(this);
}

// --- Income Statement (maps backend IncomeStatement) ---
@JsonSerializable()
class IncomeStatementItem {
  final String category;
  final double amount;

  IncomeStatementItem({required this.category, required this.amount});

  factory IncomeStatementItem.fromJson(Map<String, dynamic> json) => _$IncomeStatementItemFromJson(json);
  Map<String, dynamic> toJson() => _$IncomeStatementItemToJson(this);
}

@JsonSerializable(explicitToJson: true)
class IncomeStatement {
  @JsonKey(name: 'start_date')
  final DateTime startDate;
  @JsonKey(name: 'end_date')
  final DateTime endDate;
  final List<IncomeStatementItem> revenues;
  final List<IncomeStatementItem> expenses;
  @JsonKey(name: 'net_income')
  final double netIncome;

  IncomeStatement({
    required this.startDate,
    required this.endDate,
    required this.revenues,
    required this.expenses,
    required this.netIncome,
  });

  factory IncomeStatement.fromJson(Map<String, dynamic> json) => _$IncomeStatementFromJson(json);
  Map<String, dynamic> toJson() => _$IncomeStatementToJson(this);
}

// --- Balance Sheet (maps backend BalanceSheet) ---
@JsonSerializable()
class BalanceSheetItem {
  final String category;
  final double amount;

  BalanceSheetItem({required this.category, required this.amount});

  factory BalanceSheetItem.fromJson(Map<String, dynamic> json) => _$BalanceSheetItemFromJson(json);
  Map<String, dynamic> toJson() => _$BalanceSheetItemToJson(this);
}

@JsonSerializable(explicitToJson: true)
class BalanceSheet {
  @JsonKey(name: 'as_of_date')
  final DateTime asOfDate;
  final List<BalanceSheetItem> assets;
  final List<BalanceSheetItem> liabilities;
  final List<BalanceSheetItem> equity;
  @JsonKey(name: 'total_assets')
  final double totalAssets;
  @JsonKey(name: 'total_liabilities_equity')
  final double totalLiabilitiesEquity;

  BalanceSheet({
    required this.asOfDate,
    required this.assets,
    required this.liabilities,
    required this.equity,
    required this.totalAssets,
    required this.totalLiabilitiesEquity,
  });

  factory BalanceSheet.fromJson(Map<String, dynamic> json) => _$BalanceSheetFromJson(json);
  Map<String, dynamic> toJson() => _$BalanceSheetToJson(this);
}

// --- Cash Flow Statement (mobile view models) ---
@JsonSerializable()
class CashFlowActivity {
  final String description;
  final double amount;

  CashFlowActivity({required this.description, required this.amount});

  factory CashFlowActivity.fromJson(Map<String, dynamic> json) => _$CashFlowActivityFromJson(json);
  Map<String, dynamic> toJson() => _$CashFlowActivityToJson(this);
}

@JsonSerializable(explicitToJson: true)
class CashFlowSection {
  final String title;
  final List<CashFlowActivity> activities;

  /// Net cash effect of this section (sum of activity amounts).
  double get netCash =>
      activities.fold(0.0, (sum, activity) => sum + activity.amount);

  CashFlowSection({required this.title, required this.activities});

  factory CashFlowSection.fromJson(Map<String, dynamic> json) => _$CashFlowSectionFromJson(json);
  Map<String, dynamic> toJson() => _$CashFlowSectionToJson(this);
}

@JsonSerializable(explicitToJson: true)
class CashFlowStatement {
  @JsonKey(name: 'start_date')
  final DateTime startDate;
  @JsonKey(name: 'end_date')
  final DateTime endDate;
  @JsonKey(name: 'report_date')
  final DateTime reportDate;
  @JsonKey(name: 'operating_activities')
  final CashFlowSection operatingActivities;
  @JsonKey(name: 'investing_activities')
  final CashFlowSection investingActivities;
  @JsonKey(name: 'financing_activities')
  final CashFlowSection financingActivities;
  @JsonKey(name: 'net_increase_decrease_in_cash')
  final double netIncreaseDecreaseInCash;
  @JsonKey(name: 'beginning_cash_balance')
  final double beginningCashBalance;
  @JsonKey(name: 'ending_cash_balance')
  final double endingCashBalance;
  @JsonKey(name: 'net_income')
  final double netIncome;

  CashFlowStatement({
    required this.startDate,
    required this.endDate,
    required this.reportDate,
    required this.operatingActivities,
    required this.investingActivities,
    required this.financingActivities,
    required this.netIncreaseDecreaseInCash,
    required this.beginningCashBalance,
    required this.endingCashBalance,
    required this.netIncome,
  });

  factory CashFlowStatement.fromJson(Map<String, dynamic> json) => _$CashFlowStatementFromJson(json);
  Map<String, dynamic> toJson() => _$CashFlowStatementToJson(this);
}

// --- Ledger (mobile view model over backend LedgerReport) ---
@JsonSerializable()
class LedgerAccountBalance {
  @JsonKey(name: 'account_number')
  final String accountNumber;
  @JsonKey(name: 'account_name')
  final String accountName;
  @JsonKey(name: 'account_type')
  final String accountType;
  @JsonKey(name: 'normal_balance')
  final String normalBalance;
  @JsonKey(name: 'end_balance')
  final double currentBalance;

  LedgerAccountBalance({
    required this.accountNumber,
    required this.accountName,
    this.accountType = 'unknown',
    required this.normalBalance,
    required this.currentBalance,
  });

  factory LedgerAccountBalance.fromJson(Map<String, dynamic> json) => _$LedgerAccountBalanceFromJson(json);
  Map<String, dynamic> toJson() => _$LedgerAccountBalanceToJson(this);
}
