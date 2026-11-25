// What the Book's rules did, as the engine recorded it.
//
// Automation runs in the background: a due rule is claimed and run by the worker, and nobody starts or schedules anything
// from a screen (ADR-013). What a person is owed is the outcome, so this reads the runs the Book made and says what each
// one posted or why it stopped, in words.
//
// A run's record names its rule by id, so the rules are read once to put the rule's own name beside its runs. A rule since
// deleted leaves its runs behind, and those say the id they were filed under rather than showing a blank.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One step of a run, as the engine took it.
class RunStep {
  const RunStep({
    required this.action,
    required this.name,
    required this.done,
    required this.period,
    required this.charged,
    required this.alreadyCharged,
    required this.reason,
  });

  /// The engine's name for what the step does, e.g. `post_depreciation`.
  final String action;

  /// The step's own name, when the rule gave it one.
  final String name;

  /// False when the step was refused, which is why the run stopped.
  final bool done;

  /// The month the step worked on, when it worked on one.
  final String period;

  /// How many assets a depreciation step wrote off.
  final int charged;

  /// How many were already charged for that month, which is how a resumed run stays honest.
  final int alreadyCharged;

  /// Why the step was refused, in the engine's words.
  final String reason;

  /// What the step did, in a sentence a person reads.
  String get whatItDid {
    final said = switch (action) {
      'post_depreciation' =>
        charged == 0
            ? 'Charged no depreciation${period.isEmpty ? '' : ' for $period'}'
            : 'Charged depreciation${period.isEmpty ? '' : ' for $period'} on '
                  '$charged ${charged == 1 ? 'asset' : 'assets'}',
      'close_period' => period.isEmpty ? 'Closed the month' : 'Closed $period',
      _ => name.isNotEmpty ? name : action,
    };
    if (!done) return said;
    if (action == 'post_depreciation' && alreadyCharged > 0) {
      return '$said ($alreadyCharged already charged)';
    }
    return said;
  }

  factory RunStep.fromJson(Map<String, dynamic> json) {
    final result = json['result'];
    final body = result is Map<String, dynamic>
        ? result
        : const <String, dynamic>{};
    final assets = body['assets'];
    return RunStep(
      action: (json['action'] as String?) ?? '',
      name: (json['step_name'] as String?) ?? '',
      done: (json['status'] as String?) == 'completed',
      period: (body['period'] as String?) ?? '',
      charged:
          (body['charged'] as num?)?.toInt() ??
          (assets is List ? assets.length : 0),
      alreadyCharged: (body['already charged'] as num?)?.toInt() ?? 0,
      reason: (json['reason'] as String?) ?? '',
    );
  }
}

/// One run of one rule.
class RuleRun {
  const RuleRun({
    required this.id,
    required this.ruleName,
    required this.status,
    required this.day,
    required this.steps,
    required this.error,
  });

  final String id;

  /// The rule's own name, or the id it was filed under when the rule is gone.
  final String ruleName;

  /// `completed`, `failed` or `running`.
  final String status;

  /// The day the run finished, or the day it was scheduled for when it has not finished, as the engine wrote it.
  final String day;

  /// The same day as a person reads it.
  String get dayReads => _dayOf(day);

  final List<RunStep> steps;

  /// Why the run stopped, in the engine's words. Empty when it finished.
  final String error;

  bool get stopped => status == 'failed' || error.isNotEmpty;

  bool get stillRunning => status == 'running';

  /// What the run did, in one sentence: the steps it took, or the reason it stopped.
  String get whatItDid {
    if (stillRunning) return 'Running now.';
    if (stopped) return error.isNotEmpty ? error : 'Stopped.';
    if (steps.isEmpty) return 'Ran, and had nothing to do.';
    return steps.map((step) => step.whatItDid).join('; ');
  }

  factory RuleRun.fromJson(
    Map<String, dynamic> json,
    Map<String, String> rules,
  ) {
    final ruleId = (json['rule_id'] as String?) ?? '';
    final steps = json['step_results'];
    final finished = (json['completed_at'] as String?) ?? '';
    final scheduled = (json['scheduled_for'] as String?) ?? '';
    return RuleRun(
      id: (json['id'] as String?) ?? '',
      ruleName: rules[ruleId]?.isNotEmpty == true ? rules[ruleId]! : ruleId,
      status: (json['status'] as String?) ?? '',
      day: finished.isNotEmpty ? finished : scheduled,
      steps: steps is List
          ? steps
                .whereType<Map<String, dynamic>>()
                .map(RunStep.fromJson)
                .toList(growable: false)
          : const [],
      error: (json['error'] as String?) ?? '',
    );
  }
}

/// Reading what the Book's rules did.
class RuleOutcomesService {
  RuleOutcomesService._();

  static final RuleOutcomesService instance = RuleOutcomesService._();

  /// A second instance for a test that needs a transport of its own.
  RuleOutcomesService.fresh();

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

  /// The runs this Book made, newest first.
  ///
  /// The runs are read first and the rules only to put names beside them, so a Book whose rules were all deleted still
  /// shows what those rules did.
  Future<List<RuleRun>> read() async {
    final runs = await _get('/automation/executions');
    if (runs is! List) return const [];
    final rules = await _ruleNames();
    final read = runs
        .whereType<Map<String, dynamic>>()
        .map((run) => RuleRun.fromJson(run, rules))
        .toList();
    // newest first: a run with a day sorts above one without, and a later day above an earlier one
    read.sort((one, two) => two.day.compareTo(one.day));
    return read;
  }

  /// The Book's rules by id, so a run can carry its rule's name.
  Future<Map<String, String>> _ruleNames() async {
    final rules = await _get('/automation/rules');
    if (rules is! List) return const {};
    return {
      for (final rule in rules.whereType<Map<String, dynamic>>())
        if ((rule['id'] as String?)?.isNotEmpty == true)
          rule['id'] as String: (rule['name'] as String?) ?? '',
    };
  }

  Future<Object?> _get(String path) async {
    final answer = await _client.get(
      Uri.parse('$_base$path'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(_refusalOf(answer));
    }
    return jsonDecode(answer.body);
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
    return 'The engine refused the runs: ${answer.statusCode}.';
  }
}

const List<String> _months = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
];

/// A day as a person reads it: `3 November 2026`, or the day as it came when it cannot be read.
String _dayOf(String day) {
  final date = day.split('T').first;
  final parts = date.split('-');
  if (parts.length != 3) return day;
  final month = int.tryParse(parts[1]);
  final dayOfMonth = int.tryParse(parts[2]);
  if (month == null || dayOfMonth == null || month < 1 || month > 12)
    return day;
  return '$dayOfMonth ${_months[month - 1]} ${parts[0]}';
}
