// Stock bought on account, reaching the books through the engine.
//
// A credit purchase is not a payment: the stock arrives, the business owes the supplier, and no money has moved. It posts to
// inventory against payables, which is a different pair of accounts from a bill received (a cost against payables), so the
// two are separate routes rather than one screen with a switch nobody reads.
//
// A supplier who invoices in dollars has given a document in dollars and the books are kept in the Book's own money. The
// engine carries one into the other at the rate the purchase states; the phone sends what the supplier showed and repeats
// the answer. Nothing is multiplied here, because an app that converted a purchase itself would be a second opinion on the
// books.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// A purchase as the engine posted it.
class PurchasePosted {
  const PurchasePosted({
    required this.orderNumber,
    required this.totalAmount,
    required this.journalEntryId,
  });

  final String orderNumber;

  /// What the purchase was carried at in the Book's own money, as the engine states it.
  final String totalAmount;

  /// The entry this purchase wrote.
  final String journalEntryId;

  factory PurchasePosted.fromJson(Map<String, dynamic> json) => PurchasePosted(
    orderNumber: (json['purchase_order_number'] as String?) ?? '',
    totalAmount: (json['total_amount'] as String?) ?? '',
    journalEntryId: (json['journal_entry_id'] as String?) ?? '',
  );
}

/// Posting a credit purchase through the engine's purchases journal.
class PurchasePostingService {
  PurchasePostingService._();

  static final PurchasePostingService instance = PurchasePostingService._();

  /// A second instance for a test that needs a transport of its own.
  PurchasePostingService.fresh();

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

  /// Post a purchase the supplier raised in [currency], at [rate].
  ///
  /// [total] is in the supplier's own money, which is what the document says. [currency] empty means the Book's own money,
  /// and no rate is sent: a rate against itself is not a rate. The engine converts and keeps what the document said beside
  /// the Book's figure, so the two can never be confused later.
  Future<PurchasePosted> post({
    required String entryDate,
    required String orderNumber,
    required String vendorId,
    required ExactAmount total,
    ExactAmount? discount,
    ExactAmount? tax,
    String dueDate = '',
    String currency = '',
    String rate = '',
    String notes = '',
  }) async {
    final abroad = currency.trim().isNotEmpty;
    final answer = await _client.post(
      Uri.parse('$_base/accounting/purchases-journal'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate.trim(),
        'purchase_order_number': orderNumber.trim(),
        'vendor_id': vendorId.trim(),
        'total_amount': total.toDecimalString(),
        if (discount != null && !discount.isZero)
          'discount_amount': discount.toDecimalString(),
        if (tax != null && !tax.isZero) 'tax_amount': tax.toDecimalString(),
        if (dueDate.trim().isNotEmpty) 'due_date': dueDate.trim(),
        if (abroad) 'currency': currency.trim().toUpperCase(),
        if (abroad) 'exchange_rate': rate.trim(),
        if (notes.trim().isNotEmpty) 'notes': notes.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return PurchasePosted.fromJson(
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
    return 'The engine refused the purchase: ${answer.statusCode}.';
  }
}
