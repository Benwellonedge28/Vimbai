import '../../../core/api/amount.dart';

// Banking models mapped to the real banking-integration-service contract:
//
//   BankConnection: POST /connect, GET /connections, DELETE /connections/{id}
//   BankTransaction: GET /transactions/{connection_id}, POST .../reconcile
//
// The service is Book-scoped (the gateway injects X-Book-ID) and keeps no
// balance per connection: balance is carried per transaction (balance_after).

/// A bank account whose statement is kept in step with this Book.
///
/// The engine records what it was told: the provider (a hand-added account is the `manual` feed), the account's own
/// name, and at most the last four digits of its number. It never holds a full account number, and a field it did not
/// receive is read as absent rather than thrown over, so one older record cannot make the whole list unreadable.
class BankConnection {
  final String id;
  final String provider;
  final String accountName;
  final String last4;
  final String accountType;
  final String status;
  final String lastSyncStatus;
  final DateTime? lastSync;
  final DateTime createdAt;

  BankConnection({
    required this.id,
    this.provider = 'manual',
    required this.accountName,
    this.last4 = '',
    this.accountType = 'checking',
    this.status = 'active',
    this.lastSyncStatus = 'never',
    this.lastSync,
    required this.createdAt,
  });

  /// What the person calls this account: its name, and the last four digits when they were given.
  String get label => last4.isEmpty ? accountName : '$accountName •••• $last4';

  static String _text(Map<String, dynamic> json, String key) {
    final held = json[key];
    return held is String ? held : (held == null ? '' : '$held');
  }

  factory BankConnection.fromJson(Map<String, dynamic> json) {
    final name = _text(json, 'account_name').isNotEmpty
        ? _text(json, 'account_name')
        : _text(json, 'bank_name');
    final created = _text(json, 'created_at');
    final synced = _text(json, 'last_sync');
    return BankConnection(
      id: _text(json, 'id'),
      provider: _text(json, 'provider').isEmpty
          ? 'manual'
          : _text(json, 'provider'),
      accountName: name,
      last4: _text(json, 'account_number_last4').isEmpty
          ? _text(json, 'account_number')
          : _text(json, 'account_number_last4'),
      accountType: _text(json, 'account_type').isEmpty
          ? 'checking'
          : _text(json, 'account_type'),
      status: _text(json, 'status').isEmpty ? 'active' : _text(json, 'status'),
      lastSyncStatus: _text(json, 'last_sync_status').isEmpty
          ? 'never'
          : _text(json, 'last_sync_status'),
      lastSync: synced.isEmpty ? null : DateTime.tryParse(synced),
      createdAt: created.isEmpty
          ? DateTime.fromMillisecondsSinceEpoch(0)
          : (DateTime.tryParse(created) ??
                DateTime.fromMillisecondsSinceEpoch(0)),
    );
  }

  Map<String, dynamic> toJson() => {
    'id': id,
    'provider': provider,
    'account_name': accountName,
    'account_number_last4': last4,
    'account_type': accountType,
    'status': status,
    'last_sync_status': lastSyncStatus,
    'last_sync': lastSync?.toIso8601String(),
    'created_at': createdAt.toIso8601String(),
  };
}

class BankTransaction {
  final String id;
  final String connectionId;
  final ExactAmount amount;
  final String transactionType; // credit, debit
  final String description;
  final DateTime transactionDate;
  final ExactAmount balanceAfter;
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
    // The feed's rows carry the connection under `bank_connection_id` and the day under `date`;
    // reading only `connection_id` and `transaction_date` threw on the first row a connection returned.
    final connection = json['connection_id'] ?? json['bank_connection_id'];
    final day = json['transaction_date'] ?? json['date'];
    final status = json['status'] as String?;
    return BankTransaction(
      id: json['id'] as String,
      connectionId: (connection as String?) ?? '',
      amount: ExactAmount.fromJson(json['amount']),
      transactionType: (json['transaction_type'] as String?) ?? 'debit',
      description: (json['description'] as String?) ?? '',
      transactionDate:
          DateTime.tryParse((day as String?) ?? '') ?? DateTime(1970),
      balanceAfter: ExactAmount.fromJson(json['balance_after']),
      reconciled:
          (json['reconciled'] as bool?) ??
          (json['reconciled_at'] != null || status == 'reconciled'),
      reference:
          (json['reference'] as String?) ??
          (json['external_id'] as String?) ??
          '',
    );
  }

  Map<String, dynamic> toJson() => {
    'id': id,
    'connection_id': connectionId,
    'amount': amount.toLedgerString(),
    'transaction_type': transactionType,
    'description': description,
    'transaction_date': transactionDate.toIso8601String(),
    'balance_after': balanceAfter.toLedgerString(),
    'reconciled': reconciled,
    'reference': reference,
  };
}
