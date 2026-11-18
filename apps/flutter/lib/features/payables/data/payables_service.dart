// What a Book owes its suppliers.
//
// The engine keeps this as a purchases ledger: creditor transactions, and every supplier's balance replayed from them, so
// there is no stored balance to drift. That is why this service records transactions rather than editing a balance, and why
// nothing here adds up a total of its own - the figure a business sees is the one the ledger answered with.
//
// Amounts travel as exact decimal text, six places, and are printed through ExactAmount: a supplier balance that went
// through a floating point number could disagree with the ledger it came from by a cent, and a cent is enough to make a
// person distrust the whole page.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// The kinds of thing that move what a Book owes a supplier.
///
/// The engine decides the direction of each from its own table, so this list is only what the phone offers to pick from;
/// sending a kind the engine does not know is recorded and moves nothing, which is worse than being asked to choose again.
enum SupplierMovement {
  bill('purchase_invoice', 'Bill received'),
  debitNote('debit_note', 'Debit note (they owe you)'),
  payment('payment', 'Paid them'),
  refund('refund', 'Refund received'),
  discount('discount', 'Discount agreed');

  const SupplierMovement(this.wire, this.label);

  final String wire;
  final String label;

  /// True when the movement takes money off what is owed rather than adding to it.
  bool get reduces => this != SupplierMovement.bill;
}

/// One supplier, as the ledger lists them: what they are owed, and enough to recognise them by.
class OwedToSupplier {
  final String supplierId;
  final String name;
  final ExactAmount balance;
  final int movements;
  final String lastActivity;

  const OwedToSupplier({
    required this.supplierId,
    required this.name,
    required this.balance,
    required this.movements,
    required this.lastActivity,
  });

  factory OwedToSupplier.fromJson(Map<String, dynamic> json) => OwedToSupplier(
    supplierId: (json['creditor_id'] as String?) ?? '',
    name: (json['creditor_name'] as String?) ?? '',
    balance: ExactAmount.fromJson(json['balance']),
    movements: (json['transactions'] as num?)?.toInt() ?? 0,
    lastActivity: (json['last_activity'] as String?) ?? '',
  );
}

/// Everything the Book owes its suppliers, as the purchases ledger answers it.
class PayablesPosition {
  final List<OwedToSupplier> suppliers;
  final ExactAmount total;

  const PayablesPosition({required this.suppliers, required this.total});

  factory PayablesPosition.fromJson(Map<String, dynamic> json) {
    final rows = [
      for (final row in (json['creditors'] as List?) ?? const [])
        OwedToSupplier.fromJson(row as Map<String, dynamic>),
    ];
    // biggest debt first, which is the order a person settles them in
    rows.sort((a, b) => b.balance.compareTo(a.balance));
    return PayablesPosition(
      suppliers: rows,
      total: ExactAmount.fromJson(json['total_balance']),
    );
  }

  bool get isEmpty => suppliers.isEmpty;
}

/// What the engine says when the control account is checked against the supplier balances.
class PayablesCheck {
  final ExactAmount controlAccount;
  final ExactAmount supplierTotal;
  final ExactAmount difference;
  final bool reconciled;

  const PayablesCheck({
    required this.controlAccount,
    required this.supplierTotal,
    required this.difference,
    required this.reconciled,
  });

  factory PayablesCheck.fromJson(Map<String, dynamic> json) => PayablesCheck(
    controlAccount: ExactAmount.fromJson(json['control_account_balance']),
    supplierTotal: ExactAmount.fromJson(json['purchases_ledger_total']),
    difference: ExactAmount.fromJson(json['difference']),
    reconciled: json['reconciled'] == true,
  );
}

/// The purchases ledger doors of one Book.
class PayablesService {
  PayablesService._();

  static final PayablesService instance = PayablesService._();

  /// A second instance for a test that needs a transport of its own.
  PayablesService.fresh();

  static const String _base = AppConfig.apiUrl;
  static const String _door = '/purchases-ledger-control';

  http.Client _client = http.Client();

  /// A test calls the service against a transport it controls. Nothing in the app sets this.
  @visibleForTesting
  void useClient(http.Client client) => _client = client;

  String? _token;

  /// A test may act as a particular session; the app leaves this alone so the session's own token is used.
  @visibleForTesting
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// What the Book owes, supplier by supplier.
  Future<PayablesPosition> position() async {
    final answer = await _client.get(
      Uri.parse('$_base$_door/creditors'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return PayablesPosition.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// Record something that moves what is owed: a bill received, a debit note, a payment, a refund.
  ///
  /// Two things are refused here rather than sent. A movement of nothing changes no balance and only adds a line somebody
  /// will later wonder about. A payment bigger than the supplier's balance is refused because the ledger would faithfully
  /// accept it and quietly leave that supplier owed a negative amount, which reads as the supplier owing the business.
  Future<OwedToSupplier> record({
    required String supplierId,
    required String supplierName,
    required SupplierMovement movement,
    required ExactAmount amount,
    required ExactAmount? outstandingBefore,
    String date = '',
    String reference = '',
    String description = '',
    String dueDate = '',
  }) async {
    if (supplierId.trim().isEmpty) {
      throw Exception('Say which supplier this is for.');
    }
    if (supplierName.trim().isEmpty) {
      throw Exception(
        'Give the supplier a name, so the next person knows who.',
      );
    }
    if (amount.isZero) {
      throw Exception(
        'A movement of nothing changes nothing, so it was not recorded.',
      );
    }
    if (amount.isNegative) {
      throw Exception('An amount cannot be less than nothing.');
    }
    if (movement.reduces &&
        outstandingBefore != null &&
        amount > outstandingBefore) {
      throw Exception(
        'You only owe ${outstandingBefore.toDecimalString()} on this supplier, so '
        '${amount.toDecimalString()} cannot come off it.',
      );
    }
    final answer = await _client.post(
      Uri.parse('$_base$_door/transactions'),
      headers: _headers,
      body: jsonEncode({
        'creditor_id': supplierId,
        'creditor_name': supplierName,
        'transaction_type': movement.wire,
        'amount': amount.toLedgerString(),
        if (date.isNotEmpty) 'date': date,
        if (reference.isNotEmpty) 'reference': reference,
        if (description.isNotEmpty) 'description': description,
        // a bill carries the day it falls due, so "when you must pay" can order it; money already paid has
        // no day to fall due, so the rule lives here rather than at each caller
        if (dueDate.isNotEmpty && !movement.reduces) 'due_date': dueDate,
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    final saved = jsonDecode(answer.body) as Map<String, dynamic>;
    return OwedToSupplier(
      supplierId: (saved['creditor_id'] as String?) ?? supplierId,
      name: (saved['creditor_name'] as String?) ?? supplierName,
      // the ledger stamps the balance after the movement, so the answer says where it now stands
      balance: ExactAmount.fromJson(saved['balance']),
      movements: 1,
      lastActivity: (saved['date'] as String?) ?? date,
    );
  }

  /// Check the purchases control account against the supplier balances.
  Future<PayablesCheck> check() async {
    final answer = await _client.post(
      Uri.parse('$_base$_door/reconcile'),
      headers: _headers,
      body: jsonEncode(const <String, String>{}),
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return PayablesCheck.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }
}
