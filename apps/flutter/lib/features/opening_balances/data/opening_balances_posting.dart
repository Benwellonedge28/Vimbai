// A business's first day in its own books: what it already held and already owed before Vimbai knew about it.
//
// Every month a Book reports is measured from its first day, so that day has to be stated once and stated honestly. A
// business arriving here usually holds stock and a bank balance and owes a supplier and a lender, and its capital is what
// is left over. Those figures are the opening trial balance.
//
// What the engine decides, and this only carries: that the two sides agree, that the accounts are ones the Book already
// keeps, and that there is only ever one first day. Nothing here adds a figure up to balance a set, and nothing here dates
// the entry: the person states the day, and the engine posts it there.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One account in the opening set, and which side of the books it sits on.
class OpeningLine {
  const OpeningLine({
    required this.accountNumber,
    required this.debit,
    required this.credit,
  });

  final String accountNumber;

  /// What the business holds in this account, when it holds something.
  final String debit;

  /// Where what it holds came from: owed to someone, or the owners' own stake.
  final String credit;

  /// Which side of the books this line sits on, in one word, for a list to read.
  String get side => credit.trim().isEmpty ? 'holds' : 'came from';

  /// The figure it states, whichever side it sits on.
  String get figure => credit.trim().isEmpty ? debit : credit;

  factory OpeningLine.fromJson(Map<String, dynamic> json) => OpeningLine(
    accountNumber: (json['account_number'] as String?) ?? '',
    debit: (json['debit'] as String?) ?? '',
    credit: (json['credit'] as String?) ?? '',
  );
}

/// The opening trial balance a Book carries, or an empty one when it has never stated a first day.
class OpeningSet {
  const OpeningSet({
    required this.entryDate,
    required this.debits,
    required this.credits,
    required this.accounts,
    required this.journalEntryId,
    required this.notes,
  });

  final String entryDate;

  /// What the business holds, as the engine carried it.
  final String debits;

  /// Where it came from, as the engine carried it.
  final String credits;
  final List<OpeningLine> accounts;
  final String journalEntryId;
  final String notes;

  /// A Book either has a first day or it does not.
  bool get isStated => entryDate.trim().isNotEmpty;

  factory OpeningSet.fromJson(Map<String, dynamic> json) => OpeningSet(
    entryDate: (json['entry_date'] as String?) ?? '',
    debits: (json['debits'] as String?) ?? '',
    credits: (json['credits'] as String?) ?? '',
    accounts: [
      for (final row in (json['accounts'] as List? ?? const []))
        if (row is Map<String, dynamic>) OpeningLine.fromJson(row),
    ],
    journalEntryId: (json['journal_entry_id'] as String?) ?? '',
    notes: (json['notes'] as String?) ?? '',
  );

  static const empty = OpeningSet(
    entryDate: '',
    debits: '',
    credits: '',
    accounts: [],
    journalEntryId: '',
    notes: '',
  );
}

/// The engine's opening balances journal, as the phone reaches it.
class OpeningBalancesPostingService extends ChangeNotifier {
  OpeningBalancesPostingService.fresh({http.Client? client}) : _client = client;

  static final OpeningBalancesPostingService instance =
      OpeningBalancesPostingService.fresh();

  http.Client? _client;

  /// A test replaces the transport, so nothing here needs a server.
  void useClient(http.Client client) => _client = client;

  http.Client get _http => _client ?? http.Client();

  static const String _base = AppConfig.apiUrl;

  /// The token a person signed in with, when the service is used outside the app's own session.
  String? _token;

  /// A test, or a screen holding its own session, states whose books these are.
  void useToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// The first day this Book carries, and an empty set when it has never stated one.
  Future<OpeningSet> read() async {
    final answer = await _http.get(
      Uri.parse('$_base/accounting/opening-balances'),
      headers: _headers,
    );
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    final body = jsonDecode(answer.body);
    if (body is! Map<String, dynamic>) return OpeningSet.empty;
    return OpeningSet.fromJson(body);
  }

  /// State what the business already had and already owed on [entryDate].
  ///
  /// Each line states one side. The engine refuses a set whose sides do not agree, and refuses a second set, because a
  /// Book has one first day.
  Future<OpeningSet> post({
    required String entryDate,
    required List<OpeningLine> lines,
    String notes = '',
  }) async {
    final answer = await _http.post(
      Uri.parse('$_base/accounting/opening-balances'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate.trim(),
        'lines': [
          for (final line in lines)
            {
              'account_number': line.accountNumber.trim(),
              if (line.credit.trim().isEmpty)
                'debit': line.debit.trim()
              else
                'credit': line.credit.trim(),
            },
        ],
        if (notes.trim().isNotEmpty) 'notes': notes.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return OpeningSet.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
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
