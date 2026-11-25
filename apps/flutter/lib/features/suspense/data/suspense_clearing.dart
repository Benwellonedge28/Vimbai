// Money that could not be placed when it was recorded, moved to where it belongs.
//
// Suspense holds an amount the books could not put anywhere at the time: money received from somebody not yet known, or a
// payment made before it was clear what it was for. It sits there as a credit when the business was paid and as a debit when
// it paid, and it is a question, not an answer, until somebody says what the amount actually was.
//
// Clearing it is one entry: the account it belongs in takes its side, and suspense gives the amount up. The phone says which
// side the account takes, because that is the whole of the decision, and the engine posts the other side to suspense. It says
// plainly that nothing moves money: suspense is a bookkeeping place, not an account anybody holds.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One amount cleared out of suspense, and the entry that posted it.
class SuspenseClearing {
  const SuspenseClearing({
    required this.entryDate,
    required this.amount,
    required this.side,
    required this.accountNumber,
    required this.accountName,
    required this.reference,
    required this.journalEntryId,
  });

  final String entryDate;
  final String amount;

  /// `debit` when the account it belongs in is debited, `credit` when it is credited. Suspense takes the other side.
  final String side;
  final String accountNumber;
  final String accountName;
  final String reference;
  final String journalEntryId;

  /// The side suspense gave up, in words a person reads rather than the ledger's.
  String get suspenseTook => side == 'debit' ? 'credited' : 'debited';

  factory SuspenseClearing.fromJson(Map<String, dynamic> json) =>
      SuspenseClearing(
        entryDate: (json['entry_date'] as String?) ?? '',
        amount: (json['amount'] as String?) ?? '',
        side: (json['side'] as String?) ?? 'credit',
        accountNumber: (json['account_number'] as String?) ?? '',
        accountName: (json['account_name'] as String?) ?? '',
        reference: (json['reference'] as String?) ?? '',
        journalEntryId: (json['journal_entry_id'] as String?) ?? '',
      );
}

class SuspenseClearingService extends ChangeNotifier {
  SuspenseClearingService.fresh({http.Client? client}) : _client = client;

  static final SuspenseClearingService instance =
      SuspenseClearingService.fresh();

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

  /// Move [amount] out of suspense into [accountNumber], which takes [side] (`debit` or `credit`).
  Future<SuspenseClearing> clear({
    required String entryDate,
    required String accountNumber,
    required String amount,
    required String side,
    String reason = '',
    String suspenseAccount = '',
  }) async {
    final answer = await _http.post(
      Uri.parse('$_base/accounting/suspense/clear'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate.trim(),
        'account_number': accountNumber.trim(),
        'amount': amount.trim(),
        'side': side,
        if (reason.trim().isNotEmpty) 'reason': reason.trim(),
        if (suspenseAccount.trim().isNotEmpty)
          'suspense_account': suspenseAccount.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return SuspenseClearing.fromJson(
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
