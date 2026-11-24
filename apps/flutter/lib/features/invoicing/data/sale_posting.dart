// A sale reaching the ledger, through the engine's own sales journal.
//
// The phone does not decide which accounts a sale goes to, and it does not convert a foreign document itself: the engine
// carries a document raised in another money into the Book's money at the rate the document states, takes each named tax
// out of the total, and writes the entry. This file only carries the document there and repeats the engine's answer,
// because an app that posted its own lines would be a second opinion on the books.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// A sale as the engine posted it.
class SalePosted {
  const SalePosted({
    required this.invoiceNumber,
    required this.totalAmount,
    required this.taxAmount,
    required this.journalEntryId,
  });

  final String invoiceNumber;

  /// What the sale was carried at in the Book's own money, as the engine states it.
  final String totalAmount;

  /// The part of it that was tax.
  final String taxAmount;

  /// The entry this sale wrote, so a person can find it in the ledger.
  final String journalEntryId;

  factory SalePosted.fromJson(Map<String, dynamic> json) => SalePosted(
    invoiceNumber: (json['invoice_number'] as String?) ?? '',
    totalAmount: (json['total_amount'] as String?) ?? '',
    taxAmount: (json['tax_amount'] as String?) ?? '',
    journalEntryId: (json['journal_entry_id'] as String?) ?? '',
  );
}

/// Posting a sale through the engine's sales journal.
class SalePostingService {
  SalePostingService._();

  static final SalePostingService instance = SalePostingService._();

  /// A second instance for a test that needs a transport of its own.
  SalePostingService.fresh();

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

  /// Post the sale an invoice describes.
  ///
  /// [total] and [discount] are in the invoice's own money, which is what the customer was shown. When [currency] names
  /// another money than the Book's, the engine converts at [rate] before posting and keeps what the document said, so the
  /// phone never multiplies a rate itself. [taxes] are the taxes the Book charges, by name: the engine takes each out of
  /// the total at the rate it was defined at.
  Future<SalePosted> post({
    required String entryDate,
    required String invoiceNumber,
    required String customerId,
    required ExactAmount total,
    required List<String> taxes,
    ExactAmount? discount,
    String currency = '',
    String rate = '',
    String status = 'outstanding',
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/sales-journal'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate,
        'invoice_number': invoiceNumber,
        'customer_id': customerId,
        'total_amount': total.toDecimalString(),
        if (discount != null && !discount.isZero)
          'discount_amount': discount.toDecimalString(),
        'taxes': taxes,
        // a sale in the Book's own money names no money at all, so nothing is invented for the ordinary case
        if (currency.trim().isNotEmpty)
          'currency': currency.trim().toUpperCase(),
        if (rate.trim().isNotEmpty) 'exchange_rate': rate.trim(),
        'status': status,
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return SalePosted.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
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
    return 'The engine refused the sale: ${answer.statusCode}.';
  }
}
