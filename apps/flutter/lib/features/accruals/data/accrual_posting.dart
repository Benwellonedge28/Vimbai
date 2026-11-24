// The period-end journals, as the phone sees them: a cost used but not billed, and money paid for something not yet used.
//
// A month's profit is only true if the month carries what it really used. Two ordinary cases spoil that, and neither is a
// mistake anybody made:
//
// * the electricity for November is billed in December, so November looks better than it was and December carries a cost it
//   did not cause. November takes the cost on an estimate and owes it, and the estimate is taken back when the bill is
//   posted;
// * a year of insurance is paid in one go in January, but eleven twelfths of it buys something not yet used. It is an asset
//   that is spent down a month at a time, not January's cost.
//
// What the engine decides, and this only carries: the figures, the months and the reversal. Nothing here works out a
// share, and nothing here dates an entry on the day somebody pressed the button.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// A cost charged to a month it has not been billed for.
class Accrual {
  const Accrual({
    required this.id,
    required this.period,
    required this.expenseAccount,
    required this.accruedAccount,
    required this.amount,
    required this.description,
    required this.journalEntryId,
    required this.reversalEntryId,
    required this.reversedOn,
  });

  final String id;

  /// The month it belongs to, as `2026-11`.
  final String period;
  final String expenseAccount;
  final String accruedAccount;

  /// What the month was charged, as the engine carried it.
  final String amount;
  final String description;
  final String journalEntryId;

  /// The reversing entry, when the estimate has been taken back.
  final String reversalEntryId;

  /// The day the reversal was dated, which is the start of the month after the one it belongs to.
  final String reversedOn;

  /// Still standing means the cost is still on the month and the liability still owed: nothing has taken it back.
  bool get isStanding => reversalEntryId.trim().isEmpty;

  factory Accrual.fromJson(Map<String, dynamic> json) => Accrual(
    id: (json['id'] as String?) ?? '',
    period: (json['period'] as String?) ?? '',
    expenseAccount: (json['expense_account'] as String?) ?? '',
    accruedAccount: (json['accrued_account'] as String?) ?? '',
    amount: (json['amount'] as String?) ?? '',
    description: (json['description'] as String?) ?? '',
    journalEntryId: (json['journal_entry_id'] as String?) ?? '',
    reversalEntryId: (json['reversal_entry_id'] as String?) ?? '',
    reversedOn: (json['reversed_on'] as String?) ?? '',
  );
}

/// One month of a prepayment, as the engine split it.
class MonthShare {
  const MonthShare({required this.period, required this.amount});

  final String period;
  final String amount;

  factory MonthShare.fromJson(Map<String, dynamic> json) => MonthShare(
    period: (json['period'] as String?) ?? '',
    amount: (json['amount'] as String?) ?? '',
  );
}

/// Money already paid for something the business has not used yet.
class Prepayment {
  const Prepayment({
    required this.id,
    required this.reference,
    required this.description,
    required this.total,
    required this.months,
    required this.coversFrom,
    required this.prepaidAccount,
    required this.expenseAccount,
    required this.stillToUse,
    required this.schedule,
    required this.released,
  });

  final String id;
  final String reference;
  final String description;

  /// What was paid, in the Book's own money.
  final String total;
  final int months;
  final String coversFrom;
  final String prepaidAccount;
  final String expenseAccount;

  /// What is still to be used, as the engine states it.
  final String stillToUse;

  /// What each month costs, exactly, so the parts add back to what was paid.
  final List<MonthShare> schedule;

  /// The months already taken into the cost.
  final List<String> released;

  /// The month to release next: the earliest one not yet used up.
  String get nextMonth {
    for (final share in schedule) {
      if (!released.contains(share.period)) return share.period;
    }
    return '';
  }

  /// What that month will move, so the person sees the figure before it is posted.
  String get nextAmount {
    final month = nextMonth;
    if (month.isEmpty) return '';
    for (final share in schedule) {
      if (share.period == month) return share.amount;
    }
    return '';
  }

  bool get isUsedUp => nextMonth.isEmpty;

  factory Prepayment.fromJson(Map<String, dynamic> json) => Prepayment(
    id: (json['id'] as String?) ?? '',
    reference: (json['reference_number'] as String?) ?? '',
    description: (json['description'] as String?) ?? '',
    total: (json['total_amount'] as String?) ?? '',
    months: int.tryParse('${json['months'] ?? ''}') ?? 0,
    coversFrom: (json['covers_from'] as String?) ?? '',
    prepaidAccount: (json['prepaid_account'] as String?) ?? '',
    expenseAccount: (json['expense_account'] as String?) ?? '',
    stillToUse: (json['still_to_use'] as String?) ?? '',
    schedule: ((json['schedule'] as List?) ?? const [])
        .whereType<Map<String, dynamic>>()
        .map(MonthShare.fromJson)
        .toList(),
    released: ((json['released'] as List?) ?? const [])
        .whereType<String>()
        .toList(),
  );
}

/// Talking to the engine's accruals and prepayments journals.
class AccrualPostingService {
  AccrualPostingService._();

  static final AccrualPostingService instance = AccrualPostingService._();

  /// A second instance for a test that needs a transport of its own.
  AccrualPostingService.fresh();

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

  Future<List<Accrual>> accruals({String period = ''}) async {
    // the address is the engine's own, built rather than typed, so a period narrows the list without inventing a path
    final address = Uri.parse('$_base/accounting/accruals').replace(
      queryParameters: period.trim().isEmpty ? null : {'period': period.trim()},
    );
    final answer = await _client.get(address, headers: _headers);
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    final rows = jsonDecode(answer.body);
    if (rows is! List) return const [];
    return rows
        .whereType<Map<String, dynamic>>()
        .map(Accrual.fromJson)
        .toList();
  }

  /// Charge [period] with a cost it has used and nobody has billed.
  Future<Accrual> recordAccrual({
    required String period,
    required String expenseAccount,
    required ExactAmount amount,
    String description = '',
    String notes = '',
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/accruals'),
      headers: _headers,
      body: jsonEncode({
        'period': period.trim(),
        'expense_account': expenseAccount.trim(),
        'amount': amount.toDecimalString(),
        if (description.trim().isNotEmpty) 'description': description.trim(),
        if (notes.trim().isNotEmpty) 'notes': notes.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return Accrual.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }

  /// Take an estimate back out, by reversing entry. The estimate itself is never edited.
  Future<Accrual> reverseAccrual(String accrualId) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/accruals/reverse'),
      headers: _headers,
      body: jsonEncode({'accrual_id': accrualId}),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return Accrual.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }

  Future<List<Prepayment>> prepayments() async {
    final answer = await _client.get(
      Uri.parse('$_base/accounting/prepayments'),
      headers: _headers,
    );
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    final rows = jsonDecode(answer.body);
    if (rows is! List) return const [];
    return rows
        .whereType<Map<String, dynamic>>()
        .map(Prepayment.fromJson)
        .toList();
  }

  /// Record money that has already left, for something the business will use over [months].
  Future<Prepayment> recordPrepayment({
    required String reference,
    required ExactAmount total,
    required int months,
    required String coversFrom,
    required String paidFrom,
    required String moneyAccount,
    required String expenseAccount,
    String description = '',
    String notes = '',
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/prepayments'),
      headers: _headers,
      body: jsonEncode({
        'reference_number': reference.trim(),
        'total_amount': total.toDecimalString(),
        'months': '$months',
        'covers_from': coversFrom.trim(),
        'paid_from': paidFrom.trim(),
        'money_account': moneyAccount.trim(),
        'expense_account': expenseAccount.trim(),
        if (description.trim().isNotEmpty) 'description': description.trim(),
        if (notes.trim().isNotEmpty) 'notes': notes.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return Prepayment.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }

  /// Take one month's share out of the asset and into the cost.
  Future<Prepayment> releaseMonth({
    required String prepaymentId,
    required String period,
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/prepayments/release'),
      headers: _headers,
      body: jsonEncode({
        'prepayment_id': prepaymentId,
        'period': period.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return Prepayment.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
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
