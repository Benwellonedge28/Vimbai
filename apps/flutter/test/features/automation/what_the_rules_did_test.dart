// What the rules did.
//
// What this pins is that the screen answers "what did automation do to my books" and offers nothing to operate: a finished
// run says what it posted, a run that stopped says why in the engine's own words, and there is no switch, schedule or
// start button anywhere on it.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/automation/data/rule_outcomes.dart';
import 'package:vimbai_mobile_client/features/automation/presentation/what_the_rules_did_page.dart';

/// Stands in for the engine: it answers with the runs it was given.
class _FakeOutcomes extends RuleOutcomesService {
  _FakeOutcomes(this.runs, {this.refusal}) : super.fresh();

  final List<RuleRun> runs;

  /// When set, the engine refuses in these words instead of answering.
  final String? refusal;

  @override
  Future<List<RuleRun>> read() async {
    if (refusal != null) throw Exception(refusal);
    return runs;
  }
}

RuleRun _run({
  String id = 'run-1',
  String rule = 'Charge depreciation',
  String status = 'completed',
  String day = '2026-11-03',
  String error = '',
  List<RunStep> steps = const [
    RunStep(
      action: 'post_depreciation',
      name: '',
      done: true,
      period: '2026-11',
      charged: 3,
      alreadyCharged: 0,
      reason: '',
    ),
  ],
}) => RuleRun(
  id: id,
  ruleName: rule,
  status: status,
  day: day,
  steps: steps,
  error: error,
);

Future<void> _open(WidgetTester tester, _FakeOutcomes outcomes) async {
  tester.view.physicalSize = const Size(1000, 2000);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    MaterialApp(home: WhatTheRulesDidPage(outcomes: outcomes)),
  );
  await tester.pumpAndSettle();
}

RuleOutcomesService _serviceAnswering(
  Map<String, String> byPath, {
  List<Uri>? asked,
  Map<String, int> statuses = const {},
}) {
  final service = RuleOutcomesService.fresh();
  service.useClient(
    MockClient((request) async {
      asked?.add(request.url);
      final body = byPath[request.url.path];
      if (body == null) {
        return http.Response('{"detail": "no such address"}', 404);
      }
      return http.Response(body, statuses[request.url.path] ?? 200);
    }),
  );
  service.setAuthToken('token');
  return service;
}

void main() {
  testWidgets('a finished run says what it posted', (tester) async {
    await _open(tester, _FakeOutcomes([_run()]));

    expect(find.text('Charge depreciation'), findsOneWidget);
    expect(
      find.text('Charged depreciation for 2026-11 on 3 assets'),
      findsOneWidget,
    );
    expect(find.text('3 November 2026'), findsOneWidget);
  });

  testWidgets('a run that stopped says why, in the engine\'s words', (
    tester,
  ) async {
    await _open(
      tester,
      _FakeOutcomes([
        _run(
          rule: 'Close the month',
          status: 'failed',
          error:
              'Step close could not be taken: 2026-10 could not be closed: '
              '2026-10 is already closed.',
          steps: const [
            RunStep(
              action: 'close_period',
              name: 'close',
              done: false,
              period: '2026-10',
              charged: 0,
              alreadyCharged: 0,
              reason: '2026-10 is already closed.',
            ),
          ],
        ),
      ]),
    );

    expect(find.textContaining('2026-10 is already closed.'), findsOneWidget);
    expect(find.byIcon(Icons.error_outline), findsOneWidget);
  });

  testWidgets('a month closed by the rules reads as closed', (tester) async {
    await _open(
      tester,
      _FakeOutcomes([
        _run(
          rule: 'Close the month',
          steps: const [
            RunStep(
              action: 'close_period',
              name: '',
              done: true,
              period: '2026-10',
              charged: 0,
              alreadyCharged: 0,
              reason: '',
            ),
          ],
        ),
      ]),
    );

    expect(find.text('Closed 2026-10'), findsOneWidget);
  });

  testWidgets('a run with nothing to do says so rather than showing nothing', (
    tester,
  ) async {
    await _open(tester, _FakeOutcomes([_run(steps: const [])]));

    expect(find.text('Ran, and had nothing to do.'), findsOneWidget);
  });

  testWidgets('a rule since deleted leaves its runs, filed under its id', (
    tester,
  ) async {
    await _open(tester, _FakeOutcomes([_run(rule: '', steps: const [])]));

    expect(find.text('A rule'), findsOneWidget);
    expect(find.text('Ran, and had nothing to do.'), findsOneWidget);
  });

  testWidgets('nothing to operate: no switch, no schedule, no start', (
    tester,
  ) async {
    await _open(tester, _FakeOutcomes([_run()]));

    expect(find.byType(Switch), findsNothing);
    expect(find.byType(SwitchListTile), findsNothing);
    expect(find.textContaining('Run now'), findsNothing);
    expect(find.textContaining('Schedule'), findsNothing);
    expect(find.textContaining('Start'), findsNothing);
  });

  testWidgets('before anything has run, the screen says the rules run alone', (
    tester,
  ) async {
    await _open(tester, _FakeOutcomes(const []));

    expect(find.text('Nothing has run yet.'), findsOneWidget);
    expect(find.textContaining('The rules run on their own.'), findsOneWidget);
  });

  testWidgets(
    'a refusal to read the runs is passed on with a way to ask again',
    (tester) async {
      await _open(
        tester,
        _FakeOutcomes(const [], refusal: 'The Book could not be read.'),
      );

      expect(
        find.textContaining('The Book could not be read.'),
        findsOneWidget,
      );
      expect(find.text('Try again'), findsOneWidget);
    },
  );

  test('the runs are read, and the rule\'s own name put beside them', () async {
    final asked = <Uri>[];
    final service = _serviceAnswering({
      '/automation/executions': jsonEncode([
        {
          'id': 'run-1',
          'rule_id': 'rule-1',
          'status': 'completed',
          'completed_at': '2026-11-03T02:00:00Z',
          'step_results': [
            {
              'action': 'post_depreciation',
              'status': 'completed',
              'result': {'period': '2026-11', 'charged': 2},
            },
          ],
          'error': null,
        },
      ]),
      '/automation/rules': jsonEncode([
        {'id': 'rule-1', 'name': 'Charge depreciation'},
      ]),
    }, asked: asked);

    final runs = await service.read();

    expect(asked.map((one) => one.path), [
      '/automation/executions',
      '/automation/rules',
    ]);
    expect(runs, hasLength(1));
    expect(runs.single.ruleName, 'Charge depreciation');
    expect(
      runs.single.whatItDid,
      'Charged depreciation for 2026-11 on 2 assets',
    );
    expect(runs.single.dayReads, '3 November 2026');
    expect(runs.single.stopped, isFalse);
  });

  test('the newest run comes first', () async {
    final service = _serviceAnswering({
      '/automation/executions': jsonEncode([
        {
          'id': 'a',
          'rule_id': 'r',
          'status': 'completed',
          'completed_at': '2026-10-01',
        },
        {
          'id': 'b',
          'rule_id': 'r',
          'status': 'completed',
          'completed_at': '2026-11-02',
        },
      ]),
      '/automation/rules': jsonEncode([
        {'id': 'r', 'name': 'Charge depreciation'},
      ]),
    });

    final runs = await service.read();

    expect(runs.map((one) => one.id), ['b', 'a']);
  });

  test('a rule since deleted leaves its run filed under its id', () async {
    final service = _serviceAnswering({
      '/automation/executions': jsonEncode([
        {
          'id': 'a',
          'rule_id': 'rule-gone',
          'status': 'completed',
          'completed_at': '2026-11-02',
        },
      ]),
      '/automation/rules': '[]',
    });

    final runs = await service.read();

    expect(runs.single.ruleName, 'rule-gone');
  });

  test(
    'a refusal to read the runs is passed on in the engine\'s words',
    () async {
      final service = _serviceAnswering(
        {
          '/automation/executions':
              '{"detail": "This Book could not be read."}',
        },
        statuses: const {'/automation/executions': 422},
      );

      await expectLater(
        service.read(),
        throwsA(
          predicate(
            (error) =>
                error.toString().contains('This Book could not be read.'),
          ),
        ),
      );
    },
  );
}
