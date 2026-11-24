// A customer sending goods back, reaching the books through the engine.
//
// A sale is turned round: the customer owes the total again less what is credited, the sale is taken back, and the tax the
// invoice carried leaves both the expense it was charged to and the liability it was owed to. No money moves out of the
// business, so no bank or cash account is touched: what the customer already paid is refunded later, in the cash book, or
// netted off another invoice.
//
// The figure asked for is the gross credit the customer was given, because that is what the credit note says. The tax comes
// out of that gross rather than being added to it, exactly as it did on the sale, so the same amount is reversed as was
// charged.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// A sales return as the engine posted it.
class SalesReturnPosted {
  const SalesReturnPosted({
    required this.returnNumber,
    required this.totalAmount,
    required this.journalEntryId,
  });

  final String returnNumber;

  /// What the return was carried at in the Book's own money, as the engine states it.
  final String totalAmount;

  /// The entry this return wrote.
  final String journalEntryId;

  factory SalesReturnPosted.fromJson(Map<String, dynamic> json) =>
      SalesReturnPosted(
        returnNumber: (json['return_number'] as String?) ?? '',
        totalAmount: (json['total_amount'] as String?) ?? '',
        journalEntryId: (json['journal_entry_id'] as String?) ?? '',
      );
}

/// Posting a customer's return through the engine's sales returns journal.
class SalesReturnPostingService {
  SalesReturnPostingService._();

  static final SalesReturnPostingService instance =
      SalesReturnPostingService._();

  /// A second instance for a test that needs a transport of its own.
  SalesReturnPostingService.fresh();

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

  /// Post a return credited to [customerId] in [currency], at [rate].
  ///
  /// [total] is the gross credit in the money the customer was billed in. [currency] empty means the Book's own money, and
  /// no rate is sent: a rate against itself is not a rate.
  Future<SalesReturnPosted> post({
    required String entryDate,
    required String returnNumber,
    required String customerId,
    required ExactAmount total,
    ExactAmount? tax,
    String originalInvoice = '',
    String reason = '',
    String currency = '',
    String rate = '',
    String notes = '',
  }) async {
    final abroad = currency.trim().isNotEmpty;
    final answer = await _client.post(
      Uri.parse('$_base/accounting/sales-returns-journal'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate.trim(),
        'return_number': returnNumber.trim(),
        'customer_id': customerId.trim(),
        'total_amount': total.toDecimalString(),
        if (tax != null && !tax.isZero) 'tax_amount': tax.toDecimalString(),
        if (originalInvoice.trim().isNotEmpty)
          'original_invoice_number': originalInvoice.trim(),
        if (reason.trim().isNotEmpty) 'reason': reason.trim(),
        if (abroad) 'currency': currency.trim().toUpperCase(),
        if (abroad) 'exchange_rate': rate.trim(),
        if (notes.trim().isNotEmpty) 'notes': notes.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return SalesReturnPosted.fromJson(
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
