// Paying a bill that was raised in another money.
//
// Two things have to be true before a foreign bill can be paid honestly. The payment must be recorded in the money the
// supplier billed in, at the rate the money was worth the day it was paid, so the bank balance moves by the Book's own
// figure. And the difference between what the books carried and what actually left has to be posted, or the payable is left
// standing at a figure nobody paid.
//
// Nothing here works a rate out for itself. The bill's own currency, rate and what is still owed come from the engine, the
// rate on the day is the person's, and the difference is posted by the engine on the payable side.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// A bill the supplier raised in a money other than the Book's, and what is still owed of it.
class BillInAnotherMoney {
  const BillInAnotherMoney({
    required this.billNumber,
    required this.currency,
    required this.rate,
    required this.outstandingUnits,
    required this.outstandingCarried,
  });

  final String billNumber;

  /// The money the supplier billed in.
  final String currency;

  /// The rate the bill was carried at.
  final ExactAmount rate;

  /// How much of that money is still owed.
  final ExactAmount outstandingUnits;

  /// What that is carried at in the Book's money.
  final ExactAmount outstandingCarried;

  /// What paying the rest of it is worth in the Book's money, at [closing].
  ExactAmount worthAt(ExactAmount closing) => outstandingUnits * closing;

  factory BillInAnotherMoney.fromJson(Map<String, dynamic> json) =>
      BillInAnotherMoney(
        billNumber: (json['bill_number'] as String?) ?? '',
        currency: (json['document_currency'] as String?) ?? '',
        rate: ExactAmount.parse((json['document_rate'] as String?) ?? '0'),
        outstandingUnits: ExactAmount.parse(
          (json['outstanding_units'] as String?) ??
              (json['document_total'] as String?) ??
              '0',
        ),
        outstandingCarried: ExactAmount.parse(
          (json['outstanding_carried'] as String?) ??
              (json['total_amount'] as String?) ??
              '0',
        ),
      );
}

/// Finding the bill behind a payment, and posting what the rate did to it.
class ForeignBillPaymentService {
  ForeignBillPaymentService._();

  static final ForeignBillPaymentService instance =
      ForeignBillPaymentService._();

  /// A second instance for a test that needs a transport of its own.
  ForeignBillPaymentService.fresh();

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

  /// The bill named by [reference], if the supplier raised it in another money.
  ///
  /// A bill in the Book's own money answers nothing, because there is no rate to ask for and no difference to post: that
  /// payment is recorded exactly as it always was.
  Future<BillInAnotherMoney?> find({
    required String vendorId,
    required String reference,
  }) async {
    if (reference.trim().isEmpty) return null;
    final answer = await _client.get(
      Uri.parse(
        '$_base/accounting/vendor-bills?vendor_id=${Uri.encodeComponent(vendorId)}',
      ),
      headers: _headers,
    );
    if (answer.statusCode != 200) return null;
    final body = jsonDecode(answer.body);
    final rows = body is Map<String, dynamic>
        ? (body['entries'] as List<dynamic>? ?? const [])
        : (body as List<dynamic>);
    for (final row in rows.whereType<Map<String, dynamic>>()) {
      if ((row['bill_number'] as String?)?.trim() != reference.trim()) continue;
      final bill = BillInAnotherMoney.fromJson(row);
      if (bill.currency.trim().isEmpty) return null;
      return bill;
    }
    return null;
  }

  /// Post what the rate did to a bill when it was paid.
  ///
  /// [carried] is what the books held the debt at and [received] what actually left; the engine decides which way the
  /// difference goes, because on a bill a stronger foreign money is a loss rather than income.
  Future<void> recordDifference({
    required String entryDate,
    required String reference,
    required ExactAmount carried,
    required ExactAmount received,
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/fx-difference'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate,
        'reference_number': reference,
        'document_side': 'payable',
        'carried': carried.toDecimalString(),
        'received': received.toDecimalString(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
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
    return 'The engine refused the payment: ${answer.statusCode}.';
  }
}
