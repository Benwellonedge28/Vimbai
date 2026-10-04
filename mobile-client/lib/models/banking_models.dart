// Banking models mapped to the real banking-integration-service contract:
//
//   BankConnection: POST /connect, GET /connections, DELETE /connections/{id}
//   BankTransaction: GET /transactions/{connection_id}, POST .../reconcile
//
// The service is Book-scoped (the gateway injects X-Book-ID) and keeps no
// balance per connection: balance is carried per transaction (balance_after).

class BankConnection {
  final String id;
  final String bankName;
  final String accountNumber;
  final String accountType;
  final String status;
  final DateTime? lastSync;
  final DateTime createdAt;

  BankConnection({
    required this.id,
    required this.bankName,
    required this.accountNumber,
    required this.accountType,
    required this.status,
    this.lastSync,
    required this.createdAt,
  });

  factory BankConnection.fromJson(Map<String, dynamic> json) {
    return BankConnection(
      id: json['id'] as String,
      bankName: json['bank_name'] as String,
      accountNumber: json['account_number'] as String,
      accountType: (json['account_type'] as String?) ?? 'checking',
      status: (json['status'] as String?) ?? 'active',
      lastSync: json['last_sync'] != null ? DateTime.parse(json['last_sync'] as String) : null,
      createdAt: DateTime.parse(json['created_at'] as String),
    );
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'bank_name': bankName,
        'account_number': accountNumber,
        'account_type': accountType,
        'status': status,
        'last_sync': lastSync?.toIso8601String(),
        'created_at': createdAt.toIso8601String(),
      };
}

class BankTransaction {
  final String id;
  final String connectionId;
  final double amount;
  final String transactionType; // credit, debit
  final String description;
  final DateTime transactionDate;
  final double balanceAfter;
  final bool reconciled;
  final String reference;

  BankTransaction({
    required this.id,
    required this.connectionId,
    required this.amount,
    required this.transactionType,
    required this.description,
    required this.transactionDate,
    required this.balanceAfter,
    required this.reconciled,
    required this.reference,
  });

  factory BankTransaction.fromJson(Map<String, dynamic> json) {
    return BankTransaction(
      id: json['id'] as String,
      connectionId: json['connection_id'] as String,
      amount: (json['amount'] as num).toDouble(),
      transactionType: json['transaction_type'] as String,
      description: (json['description'] as String?) ?? '',
      transactionDate: DateTime.parse(json['transaction_date'] as String),
      balanceAfter: ((json['balance_after'] as num?) ?? 0).toDouble(),
      reconciled: (json['reconciled'] as bool?) ?? false,
      reference: (json['reference'] as String?) ?? '',
    );
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'connection_id': connectionId,
        'amount': amount,
        'transaction_type': transactionType,
        'description': description,
        'transaction_date': transactionDate.toIso8601String(),
        'balance_after': balanceAfter,
        'reconciled': reconciled,
        'reference': reference,
      };
}
