// Who owes you, on the phone.
//
// Three things are pinned here. The reading, because the figure that matters is what is still owed after the money that
// arrived, not the invoice total. The sentence, because the chasing is the work and it must name the invoice, the amount
// and the day. And the day the ageing speaks for, because an ageing with no day is a number nobody can act on.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/aged_receivables/data/aged_receivables_service.dart';
import 'package:vimbai_mobile_client/features/aged_receivables/presentation/aged_receivables_page.dart';

const _answer = '''
{"as_of":"2026-11-25","aged_from":"the day you asked for",
 "totals":{"owed":"300.000000","late":"300.000000","in_credit":"0.000000","customers":1,
           "oldest_late_days":56},
 "customers":[
   {"customer_id":"acme","customer_name":"Acme Ltd","owed":"300.000000","late":"300.000000",
    "not_due":"0.000000","1_30":"0.000000","31_60":"300.000000","61_90":"0.000000",
    "over_90":"0.000000","in_credit":"0.000000",
    "invoices":[
      {"invoice_number":"INV-1","invoice_date":"2026-09-01","due_date":"2026-09-30",
       "aged_from":"its due date","invoice_amount":"300.000000","paid":"0.000000",
       "outstanding":"300.000000","days_late":56,"band":"31_60"}],
    "reminder":"Acme Ltd owes 300.000000. The oldest unpaid invoice, INV-1 for 300.000000, fell due on 2026-09-30 and is 56 days late."}],
 "not_aged":0,
 "summary":"as of 2026-11-25, 1 customers owe 300.000000, of which 300.000000 is late; the oldest debt is 56 days old"}
''';

const _nothingOwed = '''
{"as_of":"2026-11-25","aged_from":"the newest date this Book holds",
 "totals":{"owed":"0.000000","late":"0.000000","in_credit":"0.000000","customers":0,
           "oldest_late_days":0},
 "customers":[],"not_aged":0,
 "summary":"nothing is owed as of 2026-11-25: this Book holds no unpaid invoice on that day"}
''';

const _partPaid = '''
{"as_of":"2026-11-25","aged_from":"the day you asked for",
 "totals":{"owed":"300.000000","late":"300.000000","in_credit":"0.000000","customers":1,
           "oldest_late_days":56},
 "customers":[
   {"customer_id":"acme","customer_name":"Acme Ltd","owed":"300.000000","late":"300.000000",
    "not_due":"0.000000","1_30":"0.000000","31_60":"300.000000","61_90":"0.000000",
    "over_90":"0.000000","in_credit":"0.000000",
    "invoices":[
      {"invoice_number":"INV-1","invoice_date":"2026-09-01","due_date":"2026-09-30",
       "aged_from":"its due date","invoice_amount":"500.000000","paid":"200.000000",
       "outstanding":"300.000000","days_late":56,"band":"31_60"}],
    "reminder":"Acme Ltd owes 300.000000. The oldest unpaid invoice, INV-1 for 300.000000, fell due on 2026-09-30 and is 56 days late."}],
 "not_aged":0,
 "summary":"as of 2026-11-25, 1 customers owe 300.000000, of which 300.000000 is late; the oldest debt is 56 days old"}
''';

AgedReceivablesService _serviceReturning(
  String body, {
  int status = 200,
  List<String>? asked,
}) {
  final service = AgedReceivablesService.fresh();
  service.useClient(
    MockClient((request) async {
      asked?.add(request.url.toString());
      return http.Response(body, status, headers: const {});
    }),
  );
  service.useToken('token');
  return service;
}

Future<void> _show(WidgetTester tester, AgedReceivablesService service) async {
  await tester.pumpWidget(
    MaterialApp(
      home: AgedReceivablesPage(service: service, today: '2026-11-25'),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  group('what the engine answered', () {
    test('a customer is read with what is owed and the sentence to send', () {
      final answer = AgedReceivables.fromJson(
        jsonDecode(_answer) as Map<String, dynamic>,
      );
      expect(answer.asOf, '2026-11-25');
      expect(answer.agedFrom, 'the day you asked for');
      expect(answer.owed, '300.000000');
      expect(answer.oldestLateDays, 56);
      expect(answer.nobodyOwes, isFalse);

      final acme = answer.customers.single;
      expect(acme.name, 'Acme Ltd');
      expect(acme.isInCredit, isFalse);
      expect(acme.reminder, contains('INV-1'));
      expect(acme.reminder, contains('56 days late'));
      expect(acme.invoices.single.number, 'INV-1');
      expect(acme.invoices.single.isLate, isTrue);
    });

    test('a part-paid invoice carries what is still owed, not its total', () {
      final invoice = AgedReceivables.fromJson(
        jsonDecode(_partPaid) as Map<String, dynamic>,
      ).customers.single.invoices.single;
      expect(invoice.invoiceAmount, '500.000000');
      expect(invoice.paid, '200.000000');
      expect(invoice.outstanding, '300.000000');
    });

    test('an answer with no debts says so rather than showing zeros', () {
      final answer = AgedReceivables.fromJson(
        jsonDecode(_nothingOwed) as Map<String, dynamic>,
      );
      expect(answer.nobodyOwes, isTrue);
      expect(answer.agedFrom, 'the newest date this Book holds');
    });
  });

  group('the screen', () {
    testWidgets('shows what is owed, how late, and the sentence to send', (
      tester,
    ) async {
      await _show(tester, _serviceReturning(_answer));
      expect(find.text('Who owes you'), findsOneWidget);
      expect(find.text('300.00'), findsWidgets);
      expect(find.text('Acme Ltd'), findsOneWidget);
      expect(find.textContaining('INV-1 for 300.000000'), findsOneWidget);
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

    testWidgets('a part payment is said plainly under the invoice', (
      tester,
    ) async {
      await _show(tester, _serviceReturning(_partPaid));
      expect(
        find.textContaining('Part paid: 200.00 of 500.00'),
        findsOneWidget,
      );
    });

    testWidgets('nobody owing is stated, not shown as a screen of zeros', (
      tester,
    ) async {
      await _show(tester, _serviceReturning(_nothingOwed));
      expect(find.text('Nobody owes you'), findsOneWidget);
      expect(
        find.textContaining('nothing is owed as of 2026-11-25'),
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
