// The plan on screen: what it says first, and what it refuses to say.
//
// Two failures worth holding: a plan whose comparison could not be read must never appear as "within plan", because that
// tells a person their spending was fine when nobody checked; and a Book past its plan must name the account to look at,
// not just a figure, or the sentence sends them hunting through the ledger.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/budgeting/data/spending_plan.dart';
import 'package:vimbai_mobile_client/features/budgeting/data/spending_plan_service.dart';
import 'package:vimbai_mobile_client/features/budgeting/presentation/spending_plan_page.dart';

String _plan({
  String id = 'plan-1',
  String name = '2026 plan',
  String start = '2026-01-01',
  String end = '2026-12-31',
  List<Map<String, String>> lines = const [
    {
      'category': 'Rent',
      'account_number': '6100',
      'budgeted_amount': '3600.000000',
    },
    {
      'category': 'Office costs',
      'account_number': '6200',
      'budgeted_amount': '2400.000000',
    },
  ],
}) => jsonEncode({
  'id': id,
  'name': name,
  'start_date': start,
  'end_date': end,
  'currency': '',
  'description': '',
  'status': 'active',
  'lines': lines,
  'line_count': lines.length,
  'total_budgeted': '6000.000000',
  'created_at': '2026-01-02T00:00:00Z',
});

String _varianceAnswer({
  String verdict = 'within',
  String against = '1460.000000',
  String remaining = '4540.000000',
  int over = 0,
  Map<String, String>? worst,
  List<Map<String, String>>? lines,
}) => jsonEncode({
  'id': 'plan-1',
  'name': '2026 plan',
  'start_date': '2026-01-01',
  'end_date': '2026-12-31',
  'currency': '',
  'total_budgeted': '6000.000000',
  'total_against_plan': against,
  'remaining_amount': remaining,
  'verdict': verdict,
  'accounts_over': over,
  'worst_account': worst,
  'over': const [],
  'lines':
      lines ??
      [
        {
          'category': 'Rent',
          'account_number': '6100',
          'budgeted_amount': '3600.000000',
          'against_plan_amount': '300.000000',
          'remaining_amount': '3300.000000',
          'state': 'under',
        },
        {
          'category': 'Office costs',
          'account_number': '6200',
          'budgeted_amount': '2400.000000',
          'against_plan_amount': '260.000000',
          'remaining_amount': '2140.000000',
          'state': 'under',
        },
      ],
});

/// A service answering from canned engine answers, and everything it was asked.
class _Fake {
  _Fake({
    List<String>? plans,
    String? variance,
    int plansStatus = 200,
    int varianceStatus = 200,
  }) : _plans = plans,
       _variance = variance,
       _plansStatus = plansStatus,
       _varianceStatus = varianceStatus;

  final List<String>? _plans;
  final String? _variance;
  final int _plansStatus;
  final int _varianceStatus;

  final List<String> asked = [];

  late final SpendingPlanService service = SpendingPlanService.fresh()
    ..useClient(
      MockClient((request) async {
        asked.add(request.url.path);
        if (request.url.path.endsWith('/variance')) {
          if (_varianceStatus != 200) {
            return http.Response(
              '{"detail":"the ledger could not be read"}',
              _varianceStatus,
            );
          }
          return http.Response(_variance ?? _varianceAnswer(), 200);
        }
        if (_plansStatus != 200) {
          return http.Response('{"detail":"no book"}', _plansStatus);
        }
        return http.Response('[${(_plans ?? const []).join(',')}]', 200);
      }),
    );
}

Widget _wrapped(Widget page) => MaterialApp(home: page);

void main() {
  testWidgets('a plan inside its line leads with what is left', (tester) async {
    final fake = _Fake(plans: [_plan()], variance: _varianceAnswer());

    await tester.pumpWidget(_wrapped(SpendingPlanPage(service: fake.service)));
    await tester.pumpAndSettle();

    expect(find.text('Within plan'), findsOneWidget);
    expect(find.text('4540.00 left of 6000.00 planned.'), findsOneWidget);
    expect(
      find.textContaining('1460.00 against 6000.00 planned'),
      findsOneWidget,
    );
  });

  testWidgets('a Book past its plan names the account to look at', (
    tester,
  ) async {
    final fake = _Fake(
      plans: [_plan()],
      variance: _varianceAnswer(
        verdict: 'over',
        against: '6060.000000',
        remaining: '-60.000000',
        over: 1,
        worst: {'account_number': '6200', 'past_by': '60.000000'},
        lines: [
          {
            'category': 'Office costs',
            'account_number': '6200',
            'budgeted_amount': '2400.000000',
            'against_plan_amount': '2460.000000',
            'remaining_amount': '-60.000000',
            'state': 'over',
          },
        ],
      ),
    );

    await tester.pumpWidget(_wrapped(SpendingPlanPage(service: fake.service)));
    await tester.pumpAndSettle();

    expect(find.text('Over plan'), findsOneWidget);
    expect(find.text('Over plan by 60.00 on Office costs.'), findsOneWidget);
  });

  testWidgets('a comparison that failed is a sentence, never "within plan"', (
    tester,
  ) async {
    final fake = _Fake(plans: [_plan()], varianceStatus: 500);

    await tester.pumpWidget(_wrapped(SpendingPlanPage(service: fake.service)));
    await tester.pumpAndSettle();

    expect(find.text('Within plan'), findsNothing);
    expect(find.textContaining('could not be read'), findsOneWidget);
    expect(find.textContaining('the ledger could not be read'), findsOneWidget);
  });

  testWidgets('a Book with no plan says nothing is planned, not zero spent', (
    tester,
  ) async {
    final fake = _Fake(plans: const []);

    await tester.pumpWidget(_wrapped(SpendingPlanPage(service: fake.service)));
    await tester.pumpAndSettle();

    expect(find.text('Nothing planned yet'), findsOneWidget);
    expect(find.text('Within plan'), findsNothing);
  });

  testWidgets('a plan list that failed is not shown as an empty one', (
    tester,
  ) async {
    final fake = _Fake(plansStatus: 503);

    await tester.pumpWidget(_wrapped(SpendingPlanPage(service: fake.service)));
    await tester.pumpAndSettle();

    expect(find.byType(CouldNotRead), findsOneWidget);
    expect(find.text('Nothing planned yet'), findsNothing);
  });

  testWidgets('opening a plan shows each account against its line', (
    tester,
  ) async {
    final fake = _Fake(plans: [_plan()], variance: _varianceAnswer());

    await tester.pumpWidget(_wrapped(SpendingPlanPage(service: fake.service)));
    await tester.pumpAndSettle();

    await tester.tap(find.text('2026 plan'));
    await tester.pumpAndSettle();

    expect(find.text('Account by account'), findsOneWidget);
    expect(find.text('300.00 of 3600.00 planned'), findsOneWidget);
    expect(find.text('3300.00 left'), findsOneWidget);
  });

  group('the service refuses what the engine would only hand back', () {
    late SpendingPlanService service;

    setUp(() {
      service = SpendingPlanService.fresh()
        ..useClient(MockClient((_) async => http.Response(_plan(), 201)));
    });

    PlanLine line({String account = '6100', String amount = '100'}) => PlanLine(
      category: 'Rent',
      accountNumber: account,
      budgeted: ExactAmount.tryParse(amount)!,
    );

    test('a plan with no name', () async {
      expect(
        () => service.keep(
          name: ' ',
          startDate: '2026-01-01',
          endDate: '2026-12-31',
          lines: [line()],
        ),
        throwsA(predicate((e) => '$e'.contains('Give the plan a name'))),
      );
    });

    test('a period that ends before it starts', () async {
      expect(
        () => service.keep(
          name: 'A plan',
          startDate: '2026-12-31',
          endDate: '2026-01-01',
          lines: [line()],
        ),
        throwsA(predicate((e) => '$e'.contains('cannot end before it starts'))),
      );
    });

    test('a plan with no lines', () async {
      expect(
        () => service.keep(
          name: 'A plan',
          startDate: '2026-01-01',
          endDate: '2026-12-31',
          lines: const [],
        ),
        throwsA(predicate((e) => '$e'.contains('plans nothing'))),
      );
    });

    test('a line of nothing', () async {
      expect(
        () => service.keep(
          name: 'A plan',
          startDate: '2026-01-01',
          endDate: '2026-12-31',
          lines: [line(amount: '0')],
        ),
        throwsA(predicate((e) => '$e'.contains('is nothing'))),
      );
    });

    test('two lines for one account', () async {
      expect(
        () => service.keep(
          name: 'A plan',
          startDate: '2026-01-01',
          endDate: '2026-12-31',
          lines: [line(), line()],
        ),
        throwsA(predicate((e) => '$e'.contains('planned for twice'))),
      );
    });

    test('a line naming no account', () async {
      expect(
        () => service.keep(
          name: 'A plan',
          startDate: '2026-01-01',
          endDate: '2026-12-31',
          lines: [line(account: ' ')],
        ),
        throwsA(predicate((e) => '$e'.contains('names no account'))),
      );
    });
  });
}
