// What you owe, on the phone.
//
// Three things are pinned here. The reading, because the figure that matters is what is still owed after the money that
// went out, not the bill total. The sentence, because paying is the work and it must name the bill, the amount and the
// day. And the day the ageing speaks for, because an ageing with no day is a number nobody can act on.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/aged_payables/data/aged_payables_service.dart';
import 'package:vimbai_mobile_client/features/aged_payables/presentation/aged_payables_page.dart';

const _answer = '''
{"as_of":"2026-11-25","aged_from":"the day you asked for",
 "totals":{"owed":"300.000000","late":"300.000000","in_credit":"0.000000","vendors":1,
           "oldest_late_days":56},
 "vendors":[
   {"vendor_id":"steel","vendor_name":"Steel Traders","owed":"300.000000","late":"300.000000",
    "not_due":"0.000000","1_30":"0.000000","31_60":"300.000000","61_90":"0.000000",
    "over_90":"0.000000","in_credit":"0.000000",
    "bills":[
      {"bill_number":"B-1","bill_date":"2026-09-01","due_date":"2026-09-30",
       "aged_from":"its due date","bill_amount":"300.000000","paid":"0.000000",
       "outstanding":"300.000000","days_late":56,"band":"31_60"}],
    "reminder":"Steel Traders is owed 300.000000. The oldest unpaid bill, B-1 for 300.000000, fell due on 2026-09-30 and is 56 days late."}],
 "not_aged":0,
 "summary":"300.000000 owed across 1 supplier, of which 300.000000 is late and the oldest by 56 days."}
''';

const _nothingOwed = '''
{"as_of":"2026-11-25","aged_from":"the newest date this Book holds",
 "totals":{"owed":"0.000000","late":"0.000000","in_credit":"0.000000","vendors":0,
           "oldest_late_days":0},
 "vendors":[],"not_aged":0,
 "summary":"Nothing is owed to anyone on 2026-11-25."}
''';

const _partPaid = '''
{"as_of":"2026-11-25","aged_from":"the day you asked for",
 "totals":{"owed":"300.000000","late":"300.000000","in_credit":"0.000000","vendors":1,
           "oldest_late_days":56},
 "vendors":[
   {"vendor_id":"steel","vendor_name":"Steel Traders","owed":"300.000000","late":"300.000000",
    "not_due":"0.000000","1_30":"0.000000","31_60":"300.000000","61_90":"0.000000",
    "over_90":"0.000000","in_credit":"0.000000",
    "bills":[
      {"bill_number":"B-1","bill_date":"2026-09-01","due_date":"2026-09-30",
       "aged_from":"its due date","bill_amount":"500.000000","paid":"200.000000",
       "outstanding":"300.000000","days_late":56,"band":"31_60"}],
    "reminder":"Steel Traders is owed 300.000000. The oldest unpaid bill, B-1 for 300.000000, fell due on 2026-09-30 and is 56 days late."}],
 "not_aged":0,
 "summary":"300.000000 owed across 1 supplier, of which 300.000000 is late and the oldest by 56 days."}
''';

AgedPayablesService _serviceReturning(
  String body, {
  int status = 200,
  List<String>? asked,
}) {
  final service = AgedPayablesService.fresh();
  service.useClient(
    MockClient((request) async {
      asked?.add(request.url.toString());
      return http.Response(body, status, headers: const {});
    }),
  );
  service.useToken('token');
  return service;
}

Future<void> _show(WidgetTester tester, AgedPayablesService service) async {
  await tester.pumpWidget(
    MaterialApp(
      home: AgedPayablesPage(service: service, today: '2026-11-25'),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  group('what the engine answered', () {
    test('a supplier is read with what is owed and the sentence to pay', () {
      final answer = AgedPayables.fromJson(
        jsonDecode(_answer) as Map<String, dynamic>,
      );
      expect(answer.asOf, '2026-11-25');
      expect(answer.agedFrom, 'the day you asked for');
      expect(answer.owed, '300.000000');
      expect(answer.oldestLateDays, 56);
      expect(answer.nothingOwed, isFalse);

      final steel = answer.suppliers.single;
      expect(steel.name, 'Steel Traders');
      expect(steel.isInCredit, isFalse);
      expect(steel.reminder, contains('B-1'));
      expect(steel.reminder, contains('56 days late'));
      expect(steel.bills.single.number, 'B-1');
      expect(steel.bills.single.isLate, isTrue);
    });

    test('a part-paid bill carries what is still owed, not its total', () {
      final bill = AgedPayables.fromJson(
        jsonDecode(_partPaid) as Map<String, dynamic>,
      ).suppliers.single.bills.single;
      expect(bill.billAmount, '500.000000');
      expect(bill.paid, '200.000000');
      expect(bill.outstanding, '300.000000');
    });

    test('an answer with nothing owed says so rather than showing zeros', () {
      final answer = AgedPayables.fromJson(
        jsonDecode(_nothingOwed) as Map<String, dynamic>,
      );
      expect(answer.nothingOwed, isTrue);
      expect(answer.agedFrom, 'the newest date this Book holds');
    });
  });

  group('the screen', () {
    testWidgets('shows what is owed, how late, and the sentence to send', (
      tester,
    ) async {
      await _show(tester, _serviceReturning(_answer));
      expect(find.text('What you owe'), findsOneWidget);
      expect(find.text('300.00'), findsWidgets);
      expect(find.text('Steel Traders'), findsOneWidget);
      expect(find.textContaining('B-1 for 300.000000'), findsOneWidget);
      expect(find.textContaining('is 56 days late'), findsOneWidget);
      expect(find.textContaining('Late 300.00'), findsOneWidget);
      // the day the ageing speaks for is named, with the reason it was used
      expect(
        find.textContaining(
          'Measured to 2026-11-25, which is the day you asked for.',
        ),
        findsOneWidget,
      );
    });

    testWidgets('a part payment is said plainly under the bill', (
      tester,
    ) async {
      await _show(tester, _serviceReturning(_partPaid));
      expect(
        find.textContaining('Part paid: 200.00 of 500.00'),
        findsOneWidget,
      );
    });

    testWidgets('nothing owed is stated, not shown as a screen of zeros', (
      tester,
    ) async {
      await _show(tester, _serviceReturning(_nothingOwed));
      expect(find.text('You owe nothing'), findsOneWidget);
      expect(
        find.textContaining('Nothing is owed to anyone on 2026-11-25.'),
        findsOneWidget,
      );
    });

    testWidgets('a refusal is shown in the engine\'s own words', (
      tester,
    ) async {
      await _show(
        tester,
        _serviceReturning(
          '{"error":"`25 November` is not a day written as YYYY-MM-DD"}',
          status: 422,
        ),
      );
      expect(
        find.textContaining('not a day written as YYYY-MM-DD'),
        findsOneWidget,
      );
    });

    testWidgets('the day the person is looking at is the day asked for', (
      tester,
    ) async {
      final asked = <String>[];
      await _show(tester, _serviceReturning(_answer, asked: asked));
      expect(asked.single, contains('as_of_date=2026-11-25'));
    });
  });
}
