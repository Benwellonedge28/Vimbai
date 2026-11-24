// Restating the month: what the books hold in another money, and the one decision a person makes about it.
//
// The engine knows which documents were raised in another money and what they were carried at; it does not know what the
// currency is worth at the period end, because that is a fact about the world rather than about the books. So the closing
// rate is the person's to give, one per currency, and everything else is read from the books and posted by the engine.
//
// Vimbai does not move money here either. A restatement records what the rate did to balances that are already recorded; no
// account is touched outside the ledger and nothing leaves the business.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One document raised in another money, as the books recorded it.
class ForeignDocument {
  const ForeignDocument({
    required this.reference,
    required this.kind,
    required this.date,
    required this.rate,
    required this.units,
    required this.carried,
    ExactAmount? outstandingUnits,
    ExactAmount? outstandingCarried,
  }) : outstandingUnits = outstandingUnits ?? units,
       outstandingCarried = outstandingCarried ?? carried;

  final String reference;

  /// `sale` for a document this Book raised, `bill` for one it received.
  final String kind;
  final String date;
  final ExactAmount rate;

  /// What the document said, in its own money.
  final ExactAmount units;

  /// What it was carried at, in the Book's money.
  final ExactAmount carried;

  /// How much of the foreign money is still owed, once what has arrived against this document is taken off.
  final ExactAmount outstandingUnits;

  /// What is still owed, carried at the rate the document was raised at. Allocated in proportion to what is left.
  final ExactAmount outstandingCarried;

  /// The account the balance sits in: what is owed to the business for a sale, what it owes for a bill.
  String get account => kind == 'bill' ? '2000' : '1100';

  /// The balance as the restatement must see it: a debt owed to the business is positive, a debt it owes is negative, so a
  /// stronger foreign money raises what is owed to it and raises what it owes in the same movement of the rate.
  ExactAmount get signedUnits =>
      kind == 'bill' ? outstandingUnits.negated() : outstandingUnits;
  ExactAmount get signedCarried =>
      kind == 'bill' ? outstandingCarried.negated() : outstandingCarried;

  /// What this document would be worth at [closing].
  ExactAmount restatedAt(ExactAmount closing) => kind == 'bill'
      ? outstandingUnits * closing.negated()
      : outstandingUnits * closing;

  /// What the rate would do to it: positive is a gain, negative a loss.
  ExactAmount differenceAt(ExactAmount closing) =>
      restatedAt(closing) - signedCarried;

  factory ForeignDocument.fromJson(Map<String, dynamic> json) =>
      ForeignDocument(
        reference: (json['reference'] as String?) ?? '',
        kind: (json['kind'] as String?) ?? 'sale',
        date: (json['date'] as String?) ?? '',
        rate: ExactAmount.parse((json['rate'] as String?) ?? '0'),
        units: ExactAmount.parse((json['units'] as String?) ?? '0'),
        carried: ExactAmount.parse((json['carried'] as String?) ?? '0'),
      );
}

/// Everything the Book holds in one money.
class CurrencyHolding {
  const CurrencyHolding({
    required this.currency,
    required this.units,
    required this.carried,
    required this.documents,
  });

  final String currency;
  final ExactAmount units;
  final ExactAmount carried;
  final List<ForeignDocument> documents;

  /// What the rate would do to every document held, at [closing].
  ExactAmount differenceAt(ExactAmount closing) => documents.fold(
    ExactAmount.zero,
    (sum, doc) => sum + doc.differenceAt(closing),
  );

  factory CurrencyHolding.fromJson(Map<String, dynamic> json) =>
      CurrencyHolding(
        currency: (json['currency'] as String?) ?? '',
        units: ExactAmount.parse((json['units'] as String?) ?? '0'),
        carried: ExactAmount.parse((json['carried'] as String?) ?? '0'),
        documents: (json['documents'] as List<dynamic>? ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(ForeignDocument.fromJson)
            .toList(),
      );
}

/// What the books hold in other money, and what the route is not.
class ForeignHoldings {
  const ForeignHoldings({required this.currencies, required this.note});

  final List<CurrencyHolding> currencies;

  /// The engine's own words about what this list means, repeated rather than paraphrased.
  final String note;

  bool get isEmpty => currencies.isEmpty;

  factory ForeignHoldings.fromJson(Map<String, dynamic> json) =>
      ForeignHoldings(
        currencies: (json['currencies'] as List<dynamic>? ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(CurrencyHolding.fromJson)
            .toList(),
        note: (json['note'] as String?) ?? '',
      );
}

/// One balance handed to the engine to restate.
class RestatementBalance {
  const RestatementBalance({
    required this.account,
    required this.units,
    required this.carried,
  });

  final String account;
  final ExactAmount units;
  final ExactAmount carried;

  Map<String, Object> toJson() => {
    'account': account,
    'units': units.toDecimalString(),
    'carried': carried.toDecimalString(),
  };
}

/// A restatement as the engine posted it.
class RestatementPosted {
  const RestatementPosted({
    required this.period,
    required this.gain,
    required this.loss,
    required this.restated,
    required this.journalEntryId,
    required this.note,
  });

  final String period;
  final ExactAmount gain;
  final ExactAmount loss;

  /// How many balances were restated.
  final int restated;
  final String journalEntryId;
  final String note;

  bool get isQuiet => restated == 0;

  factory RestatementPosted.fromJson(Map<String, dynamic> json) =>
      RestatementPosted(
        period: (json['period'] as String?) ?? '',
        gain: ExactAmount.parse((json['exchange_gain'] as String?) ?? '0'),
        loss: ExactAmount.parse((json['exchange_loss'] as String?) ?? '0'),
        restated: (json['restated'] as num?)?.toInt() ?? 0,
        journalEntryId: (json['journal_entry_id'] as String?) ?? '',
        note: (json['note'] as String?) ?? '',
      );
}

/// Reading what is held in another money, and posting the month's restatement.
class FxRestatementService {
  FxRestatementService._();

  static final FxRestatementService instance = FxRestatementService._();

  /// A second instance for a test that needs a transport of its own.
  FxRestatementService.fresh();

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

  /// The documents this Book holds in another money, by currency.
  Future<ForeignHoldings> holdings() async {
    final answer = await _client.get(
      Uri.parse('$_base/accounting/fx-balances'),
      headers: _headers,
    );
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    return ForeignHoldings.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// Restate one currency's balances at [closingRate], dated on [entryDate].
  ///
  /// The engine refuses a month that has been closed, by name, and says so rather than posting into a month somebody has
  /// already reported on.
  Future<RestatementPosted> restate({
    required String entryDate,
    required String closingRate,
    required List<RestatementBalance> balances,
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/fx-revaluation'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate,
        'closing_rate': closingRate,
        'balances': [for (final balance in balances) balance.toJson()],
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return RestatementPosted.fromJson(
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
    return 'The engine refused the restatement: ${answer.statusCode}.';
  }
}
