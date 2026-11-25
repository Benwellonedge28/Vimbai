// A bank statement matched against the cash book, on the phone.
//
// A person records what they did - a receipt, a payment, a cheque written - and the statement arrives later from the
// bank. Reading the two side by side is the job, and it is the job the engine already does: it matches each statement
// line to at most one posted entry on the exact amount and the same side, then proves the difference between the two
// balances is exactly the lines each side holds alone.
//
// Nothing is matched here. A phone that decided what matched would disagree with the proof it prints, so this carries the
// statement and repeats the engine's answer, including the figures that prove nothing is left unexplained.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One line as it appears on the statement the bank sent.
class StatementLine {
  const StatementLine({
    required this.date,
    required this.amount,
    required this.isIn,
    this.reference = '',
    this.description = '',
  });

  /// The day the bank moved the money, written `YYYY-MM-DD`.
  final String date;
  final String amount;

  /// Whether the money came in or went out.
  final bool isIn;
  final String reference;
  final String description;

  Map<String, dynamic> toJson() => {
    'date': date,
    'amount': amount,
    'side': isIn ? 'in' : 'out',
    if (reference.trim().isNotEmpty) 'reference': reference.trim(),
    if (description.trim().isNotEmpty) 'description': description.trim(),
  };
}

/// One line the engine matched, and the entry it matched.
class MatchedLine {
  const MatchedLine({
    required this.line,
    required this.amount,
    required this.isIn,
    required this.statementDate,
    required this.entryDate,
    required this.entryReference,
    required this.entryDescription,
  });

  final int line;
  final String amount;
  final bool isIn;
  final String statementDate;
  final String entryDate;
  final String entryReference;
  final String entryDescription;

  factory MatchedLine.fromJson(Map<String, dynamic> json) => MatchedLine(
    line: (json['statement_line'] as num?)?.toInt() ?? 0,
    amount: (json['amount'] as String?) ?? '',
    isIn: (json['side'] as String?) == 'in',
    statementDate: (json['statement_date'] as String?) ?? '',
    entryDate: (json['entry_date'] as String?) ?? '',
    entryReference: (json['entry_reference'] as String?) ?? '',
    entryDescription: (json['entry_description'] as String?) ?? '',
  );
}

/// A line only one side holds: on the statement and not in the books, or the other way round.
class LoneLine {
  const LoneLine({
    required this.amount,
    required this.isIn,
    required this.date,
    required this.reference,
    required this.description,
  });

  final String amount;
  final bool isIn;
  final String date;
  final String reference;
  final String description;

  factory LoneLine.fromJson(Map<String, dynamic> json) => LoneLine(
    amount: (json['amount'] as String?) ?? '',
    isIn: (json['side'] as String?) == 'in',
    date: ((json['date'] ?? json['entry_date']) as String?) ?? '',
    reference:
        ((json['reference'] ?? json['entry_reference']) as String?) ?? '',
    description:
        ((json['description'] ?? json['entry_description']) as String?) ?? '',
  );
}

/// What the engine answered.
class Reconciliation {
  const Reconciliation({
    required this.account,
    required this.accountName,
    required this.statementBalance,
    required this.bookBalance,
    required this.difference,
    required this.unexplainedDifference,
    required this.agrees,
    required this.summary,
    required this.matched,
    required this.onStatementOnly,
    required this.inBooksOnly,
  });

  final String account;
  final String accountName;
  final String statementBalance;
  final String bookBalance;

  /// The statement's figure less the books'.
  final String difference;

  /// What is left of that difference once the lines each side holds alone are taken into account.
  final String unexplainedDifference;

  /// Whether nothing is left unexplained.
  final bool agrees;

  /// The engine's own sentence for the whole reconciliation.
  final String summary;
  final List<MatchedLine> matched;
  final List<LoneLine> onStatementOnly;
  final List<LoneLine> inBooksOnly;

  factory Reconciliation.fromJson(Map<String, dynamic> json) => Reconciliation(
    account: (json['bank_account'] as String?) ?? '',
    accountName: (json['account_name'] as String?) ?? '',
    statementBalance: (json['statement_balance'] as String?) ?? '',
    bookBalance: (json['book_balance'] as String?) ?? '',
    difference: (json['difference'] as String?) ?? '',
    unexplainedDifference: (json['unexplained_difference'] as String?) ?? '',
    agrees: json['agrees'] == true,
    summary: (json['summary'] as String?) ?? '',
    matched: [
      for (final row in (json['matched'] as List?) ?? const [])
        if (row is Map<String, dynamic>) MatchedLine.fromJson(row),
    ],
    onStatementOnly: [
      for (final row in (json['in_statement_only'] as List?) ?? const [])
        if (row is Map<String, dynamic>) LoneLine.fromJson(row),
    ],
    inBooksOnly: [
      for (final row in (json['in_books_only'] as List?) ?? const [])
        if (row is Map<String, dynamic>) LoneLine.fromJson(row),
    ],
  );
}

class BankReconciliationService extends ChangeNotifier {
  BankReconciliationService.fresh({http.Client? client}) : _client = client;

  static final BankReconciliationService instance =
      BankReconciliationService.fresh();

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

  /// Match [lines] against what the cash book holds for [account], against the balance the statement ends on.
  Future<Reconciliation> match({
    required String account,
    required String statementBalance,
    required List<StatementLine> lines,
  }) async {
    final answer = await _http.post(
      Uri.parse('$_base/accounting/bank-reconciliation/match'),
      headers: _headers,
      body: jsonEncode({
        'bank_account': account.trim(),
        'statement_balance': statementBalance.trim(),
        'lines': [for (final line in lines) line.toJson()],
      }),
    );
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    return Reconciliation.fromJson(
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
