// A bill in another money, reaching the books through the engine.
//
// A supplier who bills in dollars has given a document in dollars, and the books are kept in the Book's own money. The
// engine carries one into the other at the rate the bill states, and writes the entry; the phone sends what the supplier
// showed and repeats the answer. Nothing is multiplied here, because an app that converted a bill itself would be a second
// opinion on the books, and a second opinion is how a set of figures stops matching what was filed.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// A bill as the engine posted it.
class VendorBillPosted {
  const VendorBillPosted({
    required this.billNumber,
    required this.totalAmount,
    required this.journalEntryId,
  });

  final String billNumber;

  /// What the bill was carried at in the Book's own money, as the engine states it.
  final String totalAmount;

  /// The entry this bill wrote.
  final String journalEntryId;

  factory VendorBillPosted.fromJson(Map<String, dynamic> json) =>
      VendorBillPosted(
        billNumber: (json['bill_number'] as String?) ?? '',
        totalAmount: (json['total_amount'] as String?) ?? '',
        journalEntryId: (json['journal_entry_id'] as String?) ?? '',
      );
}

/// Posting a bill received through the engine's vendor bills route.
class VendorBillPostingService {
  VendorBillPostingService._();

  static final VendorBillPostingService instance = VendorBillPostingService._();

  /// A second instance for a test that needs a transport of its own.
  VendorBillPostingService.fresh();

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

  /// Post a bill the supplier raised in [currency], at [rate].
  ///
  /// [total] is in the supplier's own money, which is what the bill says. The engine converts it at [rate] and keeps what
  /// the document said beside the Book's figure, so the two can never be confused later.
  Future<VendorBillPosted> post({
    required String entryDate,
    required String billNumber,
    required String vendorId,
    required ExactAmount total,
    required String currency,
    required String rate,
    String dueDate = '',
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/vendor-bills'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate,
        'bill_number': billNumber,
        'vendor_id': vendorId,
        'total_amount': total.toDecimalString(),
        'currency': currency.trim().toUpperCase(),
        'exchange_rate': rate.trim(),
        if (dueDate.trim().isNotEmpty) 'due_date': dueDate.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return VendorBillPosted.fromJson(
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
    return 'The engine refused the bill: ${answer.statusCode}.';
  }
}
