// When you must pay, held to the engine's placing.
//
// The page answers one question: in what order does the money have to leave? Three ways a page like this misleads, and each
// is pinned here. A bill with no due date is never treated as due today. A supplier who is late leads, because that is the
// only part of the figure already costing something. And a read that failed is never rendered as a Book owing nothing,
// which would tell somebody they have nothing to pay on the morning a payment was actually due.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/payables/data/due_plan.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/due_plan_page.dart';

const String _plan =
    '{"as_of":"2026-11-18","overdue":"200.000000","due_soon":"120.000000",'
    '"later":"500.000000","undated":"75.000000","overdue_suppliers":1,'
    '"next_due_date":"2026-10-31","creditors":['
    '{"creditor_id":"paperco","creditor_name":"Paper Co","owed":"700.000000",'
    '"overdue":"200.000000","due_date":"2026-10-31","open_bills":2},'
    '{"creditor_id":"inkco","creditor_name":"Ink Supplies","owed":"120.000000",'
    '"overdue":"0.000000","due_date":"2026-11-22","open_bills":1},'
    '{"creditor_id":"rentco","creditor_name":"Rent Co","owed":"575.000000",'
    '"overdue":"0.000000","due_date":"2026-12-31","open_bills":1}]}';

const String _nothing =
    '{"as_of":"2026-11-18","overdue":"0.000000","due_soon":"0.000000",'
    '"later":"0.000000","undated":"0.000000","overdue_suppliers":0,'
    '"next_due_date":"","creditors":[]}';

const String _allUndated =
    '{"as_of":"2026-11-18","overdue":"0.000000","due_soon":"0.000000",'
    '"later":"0.000000","undated":"250.000000","overdue_suppliers":0,'
    '"next_due_date":"","creditors":['
    '{"creditor_id":"loose","creditor_name":"Loose Ends","owed":"250.000000",'
    '"overdue":"0.000000","due_date":"","open_bills":1}]}';

/// A service answering from a canned engine answer, and everything it was asked.
class _Fake {
  _Fake({String? answer, int status = 200})
    : _answer = answer,
      _status = status;

  final String? _answer;
  final int _status;

  final List<http.Request> sent = [];

  late final DuePlanService service = DuePlanService.fresh()
    ..useClient(
      MockClient((request) async {
        sent.add(request);
        if (_status != 200) {
          return http.Response(
            '{"detail":"the ledger is unreachable"}',
            _status,
          );
        }
        return http.Response(_answer ?? _plan, 200);
      }),
    );
}

void main() {
  test(
    'the plan leads with what is late, and how many suppliers it is late to',
    () {
      final plan = DuePlan.fromJson(jsonDecode(_plan) as Map<String, dynamic>);
      expect(plan.headline, '200.00 is late to 1 supplier');
      expect(plan.detail, contains('120.00 within a week'));
      expect(plan.detail, contains('75.00 with no date on the bill'));
      expect(plan.total.toDecimalString(), '895.00');
    },
  );

  test('a bill with no due date is reported apart, never as due today', () {
    final plan = DuePlan.fromJson(
      jsonDecode(_allUndated) as Map<String, dynamic>,
    );
    expect(plan.overdue.toDecimalString(), '0.00');
    expect(plan.dueSoon.toDecimalString(), '0.00');
    expect(plan.undated.toDecimalString(), '250.00');
    expect(plan.headline, contains('no open bill has a due date'));
    expect(plan.suppliers.single.timing, 'No due date given');
  });

  test('a Book owing nothing says so plainly', () {
    final plan = DuePlan.fromJson(jsonDecode(_nothing) as Map<String, dynamic>);
    expect(plan.nothingOwed, isTrue);
    expect(plan.headline, 'You owe nothing to suppliers');
    expect(plan.detail, isEmpty);
  });

  test(
    'the plan is asked for as at a day, so it cannot drift through the day',
    () async {
      final fake = _Fake();
      await fake.service.plan('2026-11-18');
      expect(
        fake.sent.single.url.path,
        endsWith('/purchases-ledger-control/due'),
      );
      expect(fake.sent.single.url.queryParameters['as_of_date'], '2026-11-18');
    },
  );

  testWidgets('the page shows the buckets and the order to pay', (
    tester,
  ) async {
    final fake = _Fake();
    await tester.pumpWidget(
      MaterialApp(
        home: DuePlanPage(service: fake.service, today: '2026-11-18'),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('200.00 is late to 1 supplier'), findsOneWidget);
    expect(find.text('Already late'), findsOneWidget);
    expect(find.text('Due within a week'), findsOneWidget);
    expect(find.text('Can wait'), findsOneWidget);
    expect(find.text('No due date on the bill'), findsOneWidget);
    // the supplier who is late comes first, and each line says when it is due
    expect(find.text('Paper Co'), findsOneWidget);
    expect(find.text('200.00 was due by 2026-10-31'), findsOneWidget);
    expect(find.text('Due 2026-11-22'), findsOneWidget);
  });

  testWidgets('a Book owing nothing is not shown as a failure', (tester) async {
    final fake = _Fake(answer: _nothing);
    await tester.pumpWidget(
      MaterialApp(
        home: DuePlanPage(service: fake.service, today: '2026-11-18'),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('You owe nothing to suppliers'), findsOneWidget);
    expect(find.textContaining('could not'), findsNothing);
  });

  testWidgets('a failed read is never shown as owing nothing', (tester) async {
    final fake = _Fake(status: 500);
    await tester.pumpWidget(
      MaterialApp(
        home: DuePlanPage(service: fake.service, today: '2026-11-18'),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('You owe nothing to suppliers'), findsNothing);
    expect(find.textContaining('could not'), findsWidgets);
  });
}
