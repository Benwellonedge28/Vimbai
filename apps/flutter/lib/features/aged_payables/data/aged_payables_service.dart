// What the business owes, to whom, how late, and the sentence saying what to pay.
//
// The mirror of the money-in side. A person paying suppliers does not want a list of every bill ever received: they want
// to know who is owed, by how much, how late, and what to settle next. The engine reads both sides of the Book - the bills
// and the money that actually went out - and answers per supplier, aged from the day each bill fell due.
//
// Nothing is aged here: a phone that decided its own bands would disagree with the report it prints. The bands, the
// sentence and the day the answer was measured from all come from the engine, and this carries them.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One unpaid bill, and what is still owed on it.
class OwedBill {
  const OwedBill({
    required this.number,
    required this.billDate,
    required this.dueDate,
    required this.agedFrom,
    required this.billAmount,
    required this.paid,
    required this.outstanding,
    required this.daysLate,
    required this.band,
  });

  final String number;
  final String billDate;

  /// The day it fell due, or empty when the Book holds no due date for it.
  final String dueDate;

  /// Why it was aged from the day it was, in the engine's words.
  final String agedFrom;
  final String billAmount;
  final String paid;
  final String outstanding;

  /// How many days past the day it fell due, or zero when it has not fallen due.
  final int daysLate;

  /// `not_due`, `1_30`, `31_60`, `61_90` or `over_90`.
  final String band;

  bool get isLate => daysLate > 0;

  factory OwedBill.fromJson(Map<String, dynamic> json) => OwedBill(
    number: (json['bill_number'] as String?) ?? '',
    billDate: (json['bill_date'] as String?) ?? '',
    dueDate: (json['due_date'] as String?) ?? '',
    agedFrom: (json['aged_from'] as String?) ?? '',
    billAmount: (json['bill_amount'] as String?) ?? '',
    paid: (json['paid'] as String?) ?? '',
    outstanding: (json['outstanding'] as String?) ?? '',
    daysLate: (json['days_late'] as num?)?.toInt() ?? 0,
    band: (json['band'] as String?) ?? 'not_due',
  );
}

/// One supplier, what is owed them, and what to pay.
class OwedToSupplier {
  const OwedToSupplier({
    required this.id,
    required this.name,
    required this.owed,
    required this.late,
    required this.notDue,
    required this.inCredit,
    required this.bills,
    required this.reminder,
  });

  final String id;
  final String name;
  final String owed;
  final String late;
  final String notDue;
  final String inCredit;
  final List<OwedBill> bills;

  /// One plain sentence naming the oldest unpaid bill, its amount, the day it fell due and how late it is.
  final String reminder;

  /// Whether the supplier has been paid more than was billed, so nothing is owed and the surplus is theirs.
  bool get isInCredit => owed == '0.000000' && inCredit != '0.000000';

  factory OwedToSupplier.fromJson(Map<String, dynamic> json) => OwedToSupplier(
    id: (json['vendor_id'] as String?) ?? '',
    name: (json['vendor_name'] as String?) ?? '',
    owed: (json['owed'] as String?) ?? '',
    late: (json['late'] as String?) ?? '',
    notDue: (json['not_due'] as String?) ?? '',
    inCredit: (json['in_credit'] as String?) ?? '',
    bills: [
      for (final row in (json['bills'] as List?) ?? const [])
        if (row is Map<String, dynamic>) OwedBill.fromJson(row),
    ],
    reminder: (json['reminder'] as String?) ?? '',
  );
}

/// What the engine answered.
class AgedPayables {
  const AgedPayables({
    required this.asOf,
    required this.agedFrom,
    required this.owed,
    required this.late,
    required this.inCredit,
    required this.oldestLateDays,
    required this.notAged,
    required this.summary,
    required this.suppliers,
  });

  /// The day the ageing was measured from.
  final String asOf;

  /// Why that day was used, in the engine's words: the day asked for, or the newest date the Book holds.
  final String agedFrom;
  final String owed;
  final String late;
  final String inCredit;
  final int oldestLateDays;

  /// Documents whose date could not be read, so they were not aged. Named rather than dropped.
  final int notAged;

  /// One sentence for the whole Book.
  final String summary;
  final List<OwedToSupplier> suppliers;

  bool get nothingOwed => suppliers.isEmpty;

  factory AgedPayables.fromJson(Map<String, dynamic> json) {
    final totals = (json['totals'] as Map<String, dynamic>?) ?? const {};
    return AgedPayables(
      asOf: (json['as_of'] as String?) ?? '',
      agedFrom: (json['aged_from'] as String?) ?? '',
      owed: (totals['owed'] as String?) ?? '0.000000',
      late: (totals['late'] as String?) ?? '0.000000',
      inCredit: (totals['in_credit'] as String?) ?? '0.000000',
      oldestLateDays: (totals['oldest_late_days'] as num?)?.toInt() ?? 0,
      notAged: (json['not_aged'] as num?)?.toInt() ?? 0,
      summary: (json['summary'] as String?) ?? '',
      suppliers: [
        for (final row in (json['vendors'] as List?) ?? const [])
          if (row is Map<String, dynamic>) OwedToSupplier.fromJson(row),
      ],
    );
  }
}

class AgedPayablesService extends ChangeNotifier {
  AgedPayablesService.fresh({http.Client? client}) : _client = client;

  static final AgedPayablesService instance = AgedPayablesService.fresh();

  http.Client? _client;

  /// A test replaces the transport, so nothing here needs a server.
  void useClient(http.Client client) => _client = client;

  http.Client get _http => _client ?? http.Client();

  static const String _base = AppConfig.apiUrl;

  String? _token;

  /// A test, or a screen holding its own session, states whose books these are.
  void useToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// What this Book owes, aged to [asOf] written `YYYY-MM-DD`. With no day the engine uses the newest date the Book
  /// holds and says so.
  Future<AgedPayables> read({String? asOf}) async {
    final uri = Uri.parse('$_base/accounting/aged-payables').replace(
      queryParameters: {
        if (asOf != null && asOf.trim().isNotEmpty) 'as_of_date': asOf.trim(),
      },
    );
    final answer = await _http.get(uri, headers: _headers);
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    return AgedPayables.fromJson(
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
    return 'The engine refused this: ${answer.statusCode}.';
  }
}
