// Goods sent back to a supplier, reaching the books through the engine.
//
// A return is the purchase turned round: the supplier owes the business back what it paid, and the cost that was taken in
// comes out again. It is not a refund of money, so no bank or cash account is touched: the supplier's balance falls, and the
// money arrives later or is netted off a bill.
//
// The figure asked for is the gross credit the supplier gave, because that is what the document says. The tax it carried
// comes out of that gross rather than being added to it, exactly as it did on the bill, so the reclaim is reversed for the
// same amount it was claimed.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// A return as the engine posted it.
class PurchaseReturnPosted {
  const PurchaseReturnPosted({
    required this.returnNumber,
    required this.totalAmount,
    required this.journalEntryId,
  });

  final String returnNumber;

  /// What the return was carried at in the Book's own money, as the engine states it.
  final String totalAmount;

  /// The entry this return wrote.
  final String journalEntryId;

  factory PurchaseReturnPosted.fromJson(Map<String, dynamic> json) =>
      PurchaseReturnPosted(
        returnNumber: (json['return_number'] as String?) ?? '',
        totalAmount: (json['total_amount'] as String?) ?? '',
        journalEntryId: (json['journal_entry_id'] as String?) ?? '',
      );
}

/// Posting goods back to a supplier through the engine's purchases returns journal.
class PurchaseReturnPostingService {
  PurchaseReturnPostingService._();

  static final PurchaseReturnPostingService instance =
      PurchaseReturnPostingService._();

  /// A second instance for a test that needs a transport of its own.
  PurchaseReturnPostingService.fresh();

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

  /// Post a return the supplier credited in [currency], at [rate].
  ///
  /// [total] is the gross credit in the supplier's own money. [currency] empty means the Book's own money, and no rate is
  /// sent: a rate against itself is not a rate.
  Future<PurchaseReturnPosted> post({
    required String entryDate,
    required String returnNumber,
    required String vendorId,
    required ExactAmount total,
    ExactAmount? tax,
    String originalOrder = '',
    String reason = '',
    String currency = '',
    String rate = '',
    String notes = '',
  }) async {
    final abroad = currency.trim().isNotEmpty;
    final answer = await _client.post(
      Uri.parse('$_base/accounting/purchases-returns-journal'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate.trim(),
        'return_number': returnNumber.trim(),
        'vendor_id': vendorId.trim(),
        'total_amount': total.toDecimalString(),
        if (tax != null && !tax.isZero) 'tax_amount': tax.toDecimalString(),
        if (originalOrder.trim().isNotEmpty)
          'original_po_number': originalOrder.trim(),
        if (reason.trim().isNotEmpty) 'reason': reason.trim(),
        if (abroad) 'currency': currency.trim().toUpperCase(),
        if (abroad) 'exchange_rate': rate.trim(),
        if (notes.trim().isNotEmpty) 'notes': notes.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return PurchaseReturnPosted.fromJson(
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
    return 'The engine refused the return: ${answer.statusCode}.';
  }
}
