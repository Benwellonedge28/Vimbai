class BankAccount {
  final String? id; // Neo4j internal ID
  final String userId;
  final String bankName;
  final String accountName;
  final String accountId; // Unique ID from bank/integration
  final String accountType;
  final String currency;
  final double currentBalance;
  final bool isSynced;
  final DateTime? lastSyncedAt;
  final DateTime createdAt;
  final DateTime updatedAt;

  BankAccount({
    this.id,
    required this.userId,
    required this.bankName,
    required this.accountName,
    required this.accountId,
    required this.accountType,
    required this.currency,
    required this.currentBalance,
    required this.isSynced,
    this.lastSyncedAt,
    required this.createdAt,
    required this.updatedAt,
  });

  factory BankAccount.fromJson(Map<String, dynamic> json) {
    return BankAccount(
      id: json['id'],
      userId: json['user_id'],
      bankName: json['bank_name'],
      accountName: json['account_name'],
      accountId: json['account_id'],
      accountType: json['account_type'],
      currency: json['currency'],
      currentBalance: (json['current_balance'] as num).toDouble(),
      isSynced: json['is_synced'],
      lastSyncedAt: json['last_synced_at'] != null ? DateTime.parse(json['last_synced_at']) : null,
      createdAt: DateTime.parse(json['created_at']),
      updatedAt: DateTime.parse(json['updated_at']),
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'bank_name': bankName,
      'account_name': accountName,
      'account_id': accountId,
      'account_type': accountType,
      'currency': currency,
      'current_balance': currentBalance,
      'is_synced': isSynced,
      'last_synced_at': lastSyncedAt?.toIso8601String(),
    };
  }
}

class BankTransaction {
  final String? id; // Neo4j internal ID
  final String bankAccountId; // Corresponding BankAccount.id
  final String transactionId; // Unique ID from bank
  final DateTime date;
  final String description;
  final double amount;
  final String transactionType;
  final String? category;
  final bool reconciled;
  final DateTime createdAt;
  final DateTime updatedAt;

  BankTransaction({
    this.id,
    required this.bankAccountId,
    required this.transactionId,
    required this.date,
    required this.description,
    required this.amount,
    required this.transactionType,
    this.category,
    required this.reconciled,
    required this.createdAt,
    required this.updatedAt,
  });

  factory BankTransaction.fromJson(Map<String, dynamic> json) {
    return BankTransaction(
      id: json['id'],
      bankAccountId: json['bank_account_id'],
      transactionId: json['transaction_id'],
      date: DateTime.parse(json['date']),
      description: json['description'],
      amount: (json['amount'] as num).toDouble(),
      transactionType: json['transaction_type'],
      category: json['category'],
      reconciled: json['reconciled'],
      createdAt: DateTime.parse(json['created_at']),
      updatedAt: DateTime.parse(json['updated_at']),
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'transaction_id': transactionId,
      'date': date.toIso8601String(),
      'description': description,
      'amount': amount,
      'transaction_type': transactionType,
      'category': category,
      'reconciled': reconciled,
    };
  }
}
