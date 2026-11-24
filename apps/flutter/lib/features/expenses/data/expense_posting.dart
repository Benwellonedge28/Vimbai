// A general expense, reaching the books through the engine's expenses journal.
//
// Rent, fuel, airtime, wages, bank charges, a licence: the ordinary costs of being open, which are not a purchase of
// anything held for sale and not a supplier's bill. Two things make an expense different from the other journals:
//
// * the figure asked for is what was actually spent, tax included, because that is what the receipt says. The tax inside it
//   comes out of that figure rather than being added on top, and each tax goes to its own account because each is reclaimed
//   separately;
// * an expense that was paid moves money the business already holds, and the engine records that movement in the cash book
//   in the same act as its double entry, so the cash book and the ledger never disagree about the same payment. An expense
//   that has not been paid yet is owed instead and touches no cash at all.
//
// Vimbai does not move money: it records what already left.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// An expense as the engine recorded it.
class ExpenseRecorded {
  const ExpenseRecorded({
    required this.reference,
    required this.totalAmount,
    required this.journalEntryId,
    required this.cashEntryId,
  });

  final String reference;

  /// What the cost was carried at in the Book's own money, as the engine states it.
  final String totalAmount;

  final String journalEntryId;

  /// The cash book entry, when the expense was paid. Empty when it is still owed, because nothing left the account.
  final String cashEntryId;

  bool get wasPaid => cashEntryId.isNotEmpty;

  factory ExpenseRecorded.fromJson(Map<String, dynamic> json) =>
      ExpenseRecorded(
        reference: (json['reference_number'] as String?) ?? '',
        totalAmount: (json['total_amount'] as String?) ?? '',
        journalEntryId: (json['journal_entry_id'] as String?) ?? '',
        cashEntryId: (json['cash_entry_id'] as String?) ?? '',
      );
}

/// Posting a general expense through the engine's expenses journal.
class ExpensePostingService {
  ExpensePostingService._();

  static final ExpensePostingService instance = ExpensePostingService._();

  /// A second instance for a test that needs a transport of its own.
  ExpensePostingService.fresh();

  static const String _base = AppConfig.apiUrl;

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

  /// Record an expense of [total] in [expenseAccount], paid to [paidTo].
  ///
  /// When [paidFrom] names the cash or bank account the money left, [moneyAccount] must name the chart account it sits in:
  /// one moves the cash book, the other is what the ledger credits. [tax] is the tax inside [total], not on top of it.
  Future<ExpenseRecorded> post({
    required String entryDate,
    required String reference,
    required String expenseAccount,
    required ExactAmount total,
    String paidTo = '',
    ExactAmount? tax,
    String taxAccount = '',
    String paidFrom = '',
    String moneyAccount = '',
    String description = '',
    String currency = '',
    String rate = '',
    String notes = '',
  }) async {
    final abroad = currency.trim().isNotEmpty;
    final answer = await _client.post(
      Uri.parse('$_base/accounting/expenses-journal'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate.trim(),
        'reference_number': reference.trim(),
        'expense_account': expenseAccount.trim(),
        'total_amount': total.toDecimalString(),
        if (paidTo.trim().isNotEmpty) 'paid_to': paidTo.trim(),
        if (tax != null && !tax.isZero) 'tax_amount': tax.toDecimalString(),
        if (taxAccount.trim().isNotEmpty)
          'tax_account_number': taxAccount.trim(),
        if (paidFrom.trim().isNotEmpty) 'paid_from': paidFrom.trim(),
        if (paidFrom.trim().isNotEmpty) 'money_account': moneyAccount.trim(),
        if (description.trim().isNotEmpty) 'description': description.trim(),
        if (abroad) 'currency': currency.trim().toUpperCase(),
        if (abroad) 'exchange_rate': rate.trim(),
        if (notes.trim().isNotEmpty) 'notes': notes.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return ExpenseRecorded.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// The engine's own sentence when it refuses, rather than a status code and a wall of JSON.
  String _refusalOf(http.Response answer) {
    try {
      final body = jsonDecode(answer.body);
      if (body is Map<String, dynamic>) {
        for (final key in ['detail', 'message', 'error']) {
          final words = body[key];
          if (words is String && words.trim().isNotEmpty) return words;
        }
      }
    } catch (_) {
      // the body was not the engine's envelope, so the status is all there is to say
    }
    return 'The engine refused the expense: ${answer.statusCode}.';
  }
}
