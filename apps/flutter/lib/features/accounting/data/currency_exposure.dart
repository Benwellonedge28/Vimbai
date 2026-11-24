// What the business still holds in each money, read from the engine.
//
// A business trading in dollars needs one question answered plainly: how much of another money is still owed to me, and how
// much do I still owe? The engine already works that out by subtracting what has been settled from what was billed, so this
// reading adds no arithmetic of its own. It keeps the two directions apart, because money owed to the business and money the
// business owes are not one figure, and it never adds one money to another.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One document raised in another money, and what is still owed on it.
class ForeignDocument {
  const ForeignDocument({
    required this.reference,
    required this.kind,
    required this.date,
    required this.rate,
    required this.units,
    required this.carried,
    required this.outstandingUnits,
    required this.outstandingCarried,
  });

  final String reference;

  /// `sale` when the business is owed, `bill` when the business owes.
  final String kind;

  final String date;

  /// The rate the document was carried at.
  final ExactAmount rate;

  /// What the document said, in its own money.
  final ExactAmount units;

  /// What that is carried at in the Book's money.
  final ExactAmount carried;

  /// How much of its own money is still owed.
  final ExactAmount outstandingUnits;

  /// What that is carried at in the Book's money.
  final ExactAmount outstandingCarried;

  bool get isOwedToTheBusiness => kind == 'sale';

  bool get isSettled => outstandingUnits.isZero;

  factory ForeignDocument.fromJson(Map<String, dynamic> json) =>
      ForeignDocument(
        reference: (json['reference'] as String?) ?? '',
        kind: (json['kind'] as String?) ?? '',
        date: (json['date'] as String?) ?? '',
        rate: ExactAmount.parse((json['rate'] as String?) ?? '0'),
        units: ExactAmount.parse((json['units'] as String?) ?? '0'),
        carried: ExactAmount.parse((json['carried'] as String?) ?? '0'),
        outstandingUnits: ExactAmount.parse(
          (json['outstanding_units'] as String?) ?? '0',
        ),
        outstandingCarried: ExactAmount.parse(
          (json['outstanding_carried'] as String?) ?? '0',
        ),
      );
}

/// Everything still held in one money, and the documents behind it.
class CurrencyExposure {
  const CurrencyExposure({
    required this.currency,
    required this.units,
    required this.carried,
    required this.documents,
  });

  final String currency;

  /// Every figure in this money, still owed, added up.
  final ExactAmount units;

  /// What that is carried at in the Book's money.
  final ExactAmount carried;

  final List<ForeignDocument> documents;

  List<ForeignDocument> get owedToTheBusiness => [
    for (final document in documents)
      if (document.isOwedToTheBusiness && !document.isSettled) document,
  ];

  List<ForeignDocument> get owedByTheBusiness => [
    for (final document in documents)
      if (!document.isOwedToTheBusiness && !document.isSettled) document,
  ];

  ExactAmount get owedToTheBusinessUnits => ExactAmount.sum(
    owedToTheBusiness.map((document) => document.outstandingUnits),
  );

  ExactAmount get owedToTheBusinessCarried => ExactAmount.sum(
    owedToTheBusiness.map((document) => document.outstandingCarried),
  );

  ExactAmount get owedByTheBusinessUnits => ExactAmount.sum(
    owedByTheBusiness.map((document) => document.outstandingUnits),
  );

  ExactAmount get owedByTheBusinessCarried => ExactAmount.sum(
    owedByTheBusiness.map((document) => document.outstandingCarried),
  );

  bool get isClear => owedToTheBusiness.isEmpty && owedByTheBusiness.isEmpty;

  factory CurrencyExposure.fromJson(Map<String, dynamic> json) =>
      CurrencyExposure(
        currency: (json['currency'] as String?) ?? '',
        units: ExactAmount.parse((json['units'] as String?) ?? '0'),
        carried: ExactAmount.parse((json['carried'] as String?) ?? '0'),
        documents: (json['documents'] as List<dynamic>? ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(ForeignDocument.fromJson)
            .toList(),
      );
}

/// A settlement that named a document this Book does not hold.
class UnmatchedSettlement {
  const UnmatchedSettlement({
    required this.currency,
    required this.document,
    required this.units,
  });

  final String currency;
  final String document;
  final ExactAmount units;

  factory UnmatchedSettlement.fromJson(Map<String, dynamic> json) =>
      UnmatchedSettlement(
        currency: (json['currency'] as String?) ?? '',
        document: (json['document'] as String?) ?? '',
        units: ExactAmount.parse((json['units'] as String?) ?? '0'),
      );
}

/// What the business holds in every money other than its own.
class CurrencyExposureReport {
  const CurrencyExposureReport({
    required this.currencies,
    required this.unmatched,
  });

  final List<CurrencyExposure> currencies;
  final List<UnmatchedSettlement> unmatched;

  /// True when there is nothing at all to show: no foreign document and no payment that named one.
  bool get isQuiet => currencies.isEmpty && unmatched.isEmpty;

  factory CurrencyExposureReport.fromJson(Map<String, dynamic> json) =>
      CurrencyExposureReport(
        currencies: (json['currencies'] as List<dynamic>? ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(CurrencyExposure.fromJson)
            .toList(),
        unmatched: (json['unmatched_settlements'] as List<dynamic>? ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(UnmatchedSettlement.fromJson)
            .toList(),
      );
}

/// Reading what is held in each money, from the engine's own report.
class CurrencyExposureService {
  CurrencyExposureService._();

  static final CurrencyExposureService instance = CurrencyExposureService._();

  /// A second instance for a test that needs a transport of its own.
  CurrencyExposureService.fresh();

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

  /// Every money this Book has traded in, and what is still owed either way.
  Future<CurrencyExposureReport> read() async {
    final answer = await _client.get(
      Uri.parse('$_base/accounting/fx-balances'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(_refusalOf(answer));
    }
    return CurrencyExposureReport.fromJson(
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
    return 'The engine refused the reading: ${answer.statusCode}.';
  }
}
