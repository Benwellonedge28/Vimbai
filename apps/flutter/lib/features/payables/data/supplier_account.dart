// One supplier's account: what was billed, what was paid, and what was left after each.
//
// The engine walks that supplier's own transactions and answers the balance it reaches, so nothing here adds up a total of
// its own. A statement is a history rather than a summary: a bill that has since been paid is still on it, because that is
// what tells a person whether the figure they are being asked for is right.
//
// The closing figure is the same replay the creditors list reports for that supplier, which is why this file never
// recomputes it from the lines - a second sum here would be a second answer, and two answers to "what do we owe them" is
// exactly the kind of disagreement that makes a ledger untrustworthy.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One movement on a supplier's account, with the balance it left behind.
class SupplierAccountLine {
  final String date;
  final String kind;
  final String reference;

  /// False for a movement the ledger records but does not count, such as a discount. Shown, never silently dropped.
  final bool moves;
  final ExactAmount amount;
  final ExactAmount running;

  const SupplierAccountLine({
    required this.date,
    required this.kind,
    required this.reference,
    required this.moves,
    required this.amount,
    required this.running,
  });

  factory SupplierAccountLine.fromJson(Map<String, dynamic> json) =>
      SupplierAccountLine(
        date: (json['date'] as String?) ?? '',
        kind: (json['transaction_type'] as String?) ?? '',
        reference: (json['reference'] as String?) ?? '',
        moves: json['moves'] != false,
        amount: ExactAmount.fromJson(json['amount']),
        running: ExactAmount.fromJson(json['running']),
      );

  /// What this line was, in words a person uses.
  String get wording {
    switch (kind) {
      case 'purchase_invoice':
        return 'Bill received';
      case 'debit_note':
        return 'Debit note';
      case 'payment':
        return 'Paid them';
      case 'refund':
        return 'Refund received';
      case 'discount':
        return 'Discount agreed, moves nothing';
      default:
        return kind.isEmpty ? 'Movement' : kind.replaceAll('_', ' ');
    }
  }
}

/// A supplier's account as the ledger answers it.
class SupplierAccount {
  final String supplierId;
  final String name;
  final List<SupplierAccountLine> lines;
  final ExactAmount billed;
  final ExactAmount paid;
  final ExactAmount credited;
  final ExactAmount balance;
  final String lastActivity;

  const SupplierAccount({
    required this.supplierId,
    required this.name,
    required this.lines,
    required this.billed,
    required this.paid,
    required this.credited,
    required this.balance,
    required this.lastActivity,
  });

  factory SupplierAccount.fromJson(Map<String, dynamic> json) =>
      SupplierAccount(
        supplierId: (json['creditor_id'] as String?) ?? '',
        name: (json['creditor_name'] as String?) ?? '',
        lines: [
          for (final row in (json['lines'] as List?) ?? const [])
            SupplierAccountLine.fromJson(row as Map<String, dynamic>),
        ],
        billed: ExactAmount.fromJson(json['total_invoices']),
        paid: ExactAmount.fromJson(json['total_payments']),
        credited: ExactAmount.fromJson(json['total_debit_notes']),
        balance: ExactAmount.fromJson(json['balance']),
        lastActivity: (json['last_activity'] as String?) ?? '',
      );

  bool get isEmpty => lines.isEmpty;

  /// What to say at the top of the account.
  ///
  /// The closing figure is the engine's, so this only puts it into a sentence. A supplier owed nothing is settled rather
  /// than being described with a zero, because "you owe them 0.00" reads like a balance somebody should act on.
  String get headline {
    if (isEmpty) return 'Nothing on this account yet';
    if (balance.isZero) return 'Settled: nothing owing';
    return 'You owe ${name} ${balance.toDecimalString()}';
  }
}

/// The doors that answer for one supplier.
class SupplierAccountService {
  SupplierAccountService._();

  static final SupplierAccountService instance = SupplierAccountService._();

  /// A second instance for a test that needs a transport of its own.
  SupplierAccountService.fresh();

  static const String _base = AppConfig.apiUrl;
  static const String _door = '/purchases-ledger-control';

  http.Client _client = http.Client();

  @visibleForTesting
  void useClient(http.Client client) => _client = client;

  String? _token;

  @visibleForTesting
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// One supplier's account.
  Future<SupplierAccount> account(String supplierId) async {
    if (supplierId.trim().isEmpty) {
      throw Exception('Say which supplier this account is for.');
    }
    final answer = await _client.get(
      Uri.parse('$_base$_door/creditors/${Uri.encodeComponent(supplierId)}'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return SupplierAccount.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }
}
