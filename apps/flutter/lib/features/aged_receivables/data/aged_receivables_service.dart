// Who owes the business, how late they are, and the sentence to send them.
//
// A person chasing money does not want a list of every invoice ever raised. They want to know who is late, by how much,
// and what to say. The engine reads both sides of the Book - the invoices and the money that actually arrived - and
// answers per customer, aged from the day each invoice fell due.
//
// Nothing is aged here: a phone that decided its own bands would disagree with the letter it prints. The bands, the
// reminder sentence and the day the answer was measured from all come from the engine, and this carries them.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One unpaid invoice, and what is still owed on it.
class OwedInvoice {
  const OwedInvoice({
    required this.number,
    required this.invoiceDate,
    required this.dueDate,
    required this.agedFrom,
    required this.invoiceAmount,
    required this.paid,
    required this.outstanding,
    required this.daysLate,
    required this.band,
  });

  final String number;
  final String invoiceDate;

  /// The day it fell due, or empty when the Book holds no due date for it.
  final String dueDate;

  /// Why it was aged from the day it was, in the engine's words.
  final String agedFrom;
  final String invoiceAmount;
  final String paid;
  final String outstanding;

  /// How many days past the day it fell due, or zero when it has not fallen due.
  final int daysLate;

  /// `not_due`, `1_30`, `31_60`, `61_90` or `over_90`.
  final String band;

  bool get isLate => daysLate > 0;

  factory OwedInvoice.fromJson(Map<String, dynamic> json) => OwedInvoice(
    number: (json['invoice_number'] as String?) ?? '',
    invoiceDate: (json['invoice_date'] as String?) ?? '',
    dueDate: (json['due_date'] as String?) ?? '',
    agedFrom: (json['aged_from'] as String?) ?? '',
    invoiceAmount: (json['invoice_amount'] as String?) ?? '',
    paid: (json['paid'] as String?) ?? '',
    outstanding: (json['outstanding'] as String?) ?? '',
    daysLate: (json['days_late'] as num?)?.toInt() ?? 0,
    band: (json['band'] as String?) ?? 'not_due',
  );
}

/// One customer, what they owe, and what to say to them.
class OwedByCustomer {
  const OwedByCustomer({
    required this.id,
    required this.name,
    required this.owed,
    required this.late,
    required this.notDue,
    required this.inCredit,
    required this.invoices,
    required this.reminder,
  });

  final String id;
  final String name;
  final String owed;
  final String late;
  final String notDue;
  final String inCredit;
  final List<OwedInvoice> invoices;

  /// One plain sentence naming the oldest unpaid invoice, its amount, the day it fell due and how late it is.
  final String reminder;

  /// Whether the customer has paid more than was invoiced, so nothing is owed and the surplus is theirs.
  bool get isInCredit => owed == '0.000000' && inCredit != '0.000000';

  factory OwedByCustomer.fromJson(Map<String, dynamic> json) => OwedByCustomer(
    id: (json['customer_id'] as String?) ?? '',
    name: (json['customer_name'] as String?) ?? '',
    owed: (json['owed'] as String?) ?? '',
    late: (json['late'] as String?) ?? '',
    notDue: (json['not_due'] as String?) ?? '',
    inCredit: (json['in_credit'] as String?) ?? '',
    invoices: [
      for (final row in (json['invoices'] as List?) ?? const [])
        if (row is Map<String, dynamic>) OwedInvoice.fromJson(row),
    ],
    reminder: (json['reminder'] as String?) ?? '',
  );
}

/// What the engine answered.
class AgedReceivables {
  const AgedReceivables({
    required this.asOf,
    required this.agedFrom,
    required this.owed,
    required this.late,
    required this.inCredit,
    required this.oldestLateDays,
    required this.notAged,
    required this.summary,
    required this.customers,
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
  final List<OwedByCustomer> customers;

  bool get nobodyOwes => customers.isEmpty;

  factory AgedReceivables.fromJson(Map<String, dynamic> json) {
    final totals = (json['totals'] as Map<String, dynamic>?) ?? const {};
    return AgedReceivables(
      asOf: (json['as_of'] as String?) ?? '',
      agedFrom: (json['aged_from'] as String?) ?? '',
      owed: (totals['owed'] as String?) ?? '0.000000',
      late: (totals['late'] as String?) ?? '0.000000',
      inCredit: (totals['in_credit'] as String?) ?? '0.000000',
      oldestLateDays: (totals['oldest_late_days'] as num?)?.toInt() ?? 0,
      notAged: (json['not_aged'] as num?)?.toInt() ?? 0,
      summary: (json['summary'] as String?) ?? '',
      customers: [
        for (final row in (json['customers'] as List?) ?? const [])
          if (row is Map<String, dynamic>) OwedByCustomer.fromJson(row),
      ],
    );
  }
}

class AgedReceivablesService extends ChangeNotifier {
  AgedReceivablesService.fresh({http.Client? client}) : _client = client;

  static final AgedReceivablesService instance = AgedReceivablesService.fresh();

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

  /// Who owes this Book, aged to [asOf] written `YYYY-MM-DD`. With no day the engine uses the newest date the Book
  /// holds and says so.
  Future<AgedReceivables> read({String? asOf}) async {
    final uri = Uri.parse('$_base/accounting/aged-receivables').replace(
      queryParameters: {
        if (asOf != null && asOf.trim().isNotEmpty) 'as_of_date': asOf.trim(),
      },
    );
    final answer = await _http.get(uri, headers: _headers);
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    return AgedReceivables.fromJson(
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
