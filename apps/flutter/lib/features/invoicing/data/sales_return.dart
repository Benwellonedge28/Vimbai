// Goods coming back: the sale undone, tax and all.
//
// A return is the sale turned round, so it is not worked out here. The engine already knows which account a return goes
// to, what each tax it carried was charged to and where that tax is owed, and it writes the return document as well as
// the entry. This file only carries the document there and repeats the engine's own answer, because an app that decided
// its own accounts would be a second opinion on the books.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// A sale taken back, as the engine recorded it.
class SalesReturn {
  const SalesReturn({
    required this.returnNumber,
    required this.totalAmount,
    required this.taxAmount,
    required this.journalEntryId,
  });

  final String returnNumber;

  /// What the customer was credited, in full.
  final String totalAmount;

  /// The part of it that was tax, which has left its expense and the tax owed.
  final String taxAmount;

  /// The entry this return wrote, so a person can find it in the ledger.
  final String journalEntryId;

  factory SalesReturn.fromJson(Map<String, dynamic> json) => SalesReturn(
    returnNumber: (json['return_number'] as String?) ?? '',
    totalAmount: (json['total_amount'] as String?) ?? '',
    taxAmount: (json['tax_amount'] as String?) ?? '',
    journalEntryId: (json['journal_entry_id'] as String?) ?? '',
  );
}

/// Taking a sale back, through the engine's own returns route.
class SalesReturnService {
  SalesReturnService._();

  static final SalesReturnService instance = SalesReturnService._();

  /// A second instance for a test that needs a transport of its own.
  SalesReturnService.fresh();

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

  /// Record the return of a sale.
  ///
  /// [taxes] are the taxes the original sale carried, by name: the engine takes each one back out of the total at the
  /// rate it was charged at, which is why the phone does not do the arithmetic itself. [total] travels as exact decimal
  /// text, never as a double.
  Future<SalesReturn> record({
    required String entryDate,
    required String returnNumber,
    required String originalInvoiceNumber,
    required String customerId,
    required ExactAmount total,
    required List<String> taxes,
    String currency = '',
    String rate = '',
    String reason = '',
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/sales-returns-journal'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate,
        'return_number': returnNumber,
        'original_invoice_number': originalInvoiceNumber,
        'customer_id': customerId,
        'total_amount': total.toDecimalString(),
        'taxes': taxes,
        // a sale billed in another money is reversed in that same money, at the same rate: without them the engine would
        // hand the customer back a figure in the Book's money and reverse a dollar sale at par
        if (currency.trim().isNotEmpty)
          'currency': currency.trim().toUpperCase(),
        if (rate.trim().isNotEmpty) 'exchange_rate': rate.trim(),
        if (reason.trim().isNotEmpty) 'reason': reason.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return SalesReturn.fromJson(
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
          if (words is String && words.trim().isNotEmpty) {
            return 'The engine refused the return: ${words.trim()}';
          }
        }
      }
    } catch (_) {
      // the body was not JSON, so the answer below is the honest one
    }
    return 'The engine refused the return: ${answer.statusCode} ${answer.body}';
  }
}
