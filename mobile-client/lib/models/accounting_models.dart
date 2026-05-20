// For Chart of Accounts
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
