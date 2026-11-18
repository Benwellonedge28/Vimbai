// The spending plan's doors at the engine.
//
// A plan used to live in the phone's own store, so a Book shared with a second person arrived with no plan at all. It is
// kept with the Book now, and this service is the only way the app reaches it: list what the Book has planned, keep a new
// plan, and read one against what the ledger says was taken up.
//
// The refusals here are the ones a person can fix in front of the screen. The engine refuses the same things, and more
// besides, but sending a plan it will only hand back is a round trip that tells nobody anything.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';
import 'package:vimbai_mobile_client/features/budgeting/data/spending_plan.dart';

/// The spending plan doors of one Book.
class SpendingPlanService {
  SpendingPlanService._();

  static final SpendingPlanService instance = SpendingPlanService._();

  /// A second instance for a test that needs a transport of its own.
  SpendingPlanService.fresh();

  static const String _base = AppConfig.apiUrl;
  static const String _door = '/records/budget-register/budgets';

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

  /// Every plan this Book keeps, the most recently started first, as the engine lists them.
  Future<List<SpendingPlan>> plans() async {
    final answer = await _client.get(
      Uri.parse('$_base$_door'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    final rows = [
      for (final row in (jsonDecode(answer.body) as List?) ?? const [])
        SpendingPlan.fromJson(row as Map<String, dynamic>),
    ];
    return rows;
  }

  /// One plan as the Book keeps it.
  Future<SpendingPlan> plan(String id) async {
    final answer = await _client.get(
      Uri.parse('$_base$_door/$id'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return SpendingPlan.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// The plan against what the posted ledger says was taken up, account by account.
  ///
  /// The engine does the comparing: the ledger decides what was spent, and the plan decides what was allowed. Nothing on
  /// this side adds a journal line up, so the report cannot disagree with the ledger it reports on.
  Future<PlanVariance> variance(String id) async {
    final answer = await _client.get(
      Uri.parse('$_base$_door/$id/variance'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return PlanVariance.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// Keep a plan with the Book.
  ///
  /// Refused here, with a sentence naming the fix: a plan with no name, a period that ends before it starts, no lines at
  /// all, a line of nothing, a line naming no account, and two lines for one account.
  Future<SpendingPlan> keep({
    required String name,
    required String startDate,
    required String endDate,
    required List<PlanLine> lines,
    String description = '',
    String currency = '',
  }) async {
    if (name.trim().isEmpty) {
      throw Exception('Give the plan a name, so it can be told apart later.');
    }
    if (startDate.trim().isEmpty || endDate.trim().isEmpty) {
      throw Exception('Say which period the plan covers.');
    }
    if (endDate.trim().compareTo(startDate.trim()) < 0) {
      throw Exception(
        'The plan cannot end before it starts: it would cover no days at all.',
      );
    }
    if (lines.isEmpty) {
      throw Exception(
        'A plan with no lines plans nothing. Add what each account is meant '
        'to take up.',
      );
    }
    final seen = <String>{};
    for (final line in lines) {
      if (line.accountNumber.trim().isEmpty) {
        throw Exception('One of the lines names no account.');
      }
      if (line.budgeted.isZero) {
        throw Exception(
          'The line for ${_named(line)} is nothing. A line of nothing and a '
          'missing line read the same on the report.',
        );
      }
      if (line.budgeted.isNegative) {
        throw Exception(
          'The line for ${_named(line)} is below nothing. Say what the '
          'account is meant to take up as a positive figure.',
        );
      }
      if (!seen.add(line.accountNumber)) {
        throw Exception(
          'Account ${line.accountNumber} is planned for twice. A second line '
          'would count its plan twice while the ledger counts its spending '
          'once.',
        );
      }
    }

    final answer = await _client.post(
      Uri.parse('$_base$_door'),
      headers: _headers,
      body: jsonEncode({
        'name': name.trim(),
        'start_date': startDate.trim(),
        'end_date': endDate.trim(),
        'description': description.trim(),
        'currency': currency.trim(),
        'lines': [for (final line in lines) line.toJson()],
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_reason(answer));
    }
    return SpendingPlan.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// A plan removed from the Book. Its lines go with it, because they are part of the record.
  Future<void> remove(String id) async {
    final answer = await _client.delete(
      Uri.parse('$_base$_door/$id'),
      headers: _headers,
    );
    if (answer.statusCode != 200 && answer.statusCode != 204) {
      throw Exception(_reason(answer));
    }
  }

  /// The engine's own words, when it says why it refused.
  String _reason(http.Response answer) {
    try {
      final body = jsonDecode(answer.body);
      if (body is Map && body['error'] is String)
        return body['error'] as String;
      if (body is Map && body['detail'] is String) {
        return body['detail'] as String;
      }
    } catch (_) {
      // Not a refusal this app can read; the status is all there is to pass on.
    }
    return 'The engine answered ${answer.statusCode}.';
  }

  String _named(PlanLine line) =>
      line.category.trim().isEmpty ? line.accountNumber : line.category;
}
