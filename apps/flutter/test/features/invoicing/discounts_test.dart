// The discounts a Book gives, and the screen where a new one is declared.
//
// What is pinned here is that a discount is sent with its own kind (a percentage or a sum of money), whether it is given
// automatically, who it is for, and the figure an invoice must reach; that the answer shown afterwards is the engine's
// rather than the app's own arithmetic; and that a refusal is repeated in the engine's words.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/discounts.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/discounts_page.dart';

const _twoRules = '''
{"discounts":[
 {"id":"d1","name":"Trade terms","kind":"percent","value":"10.000000","compulsory":true,
  "all_customers":true,"customer_ids":[],"min_total":"0.000000",
  "effective_from":"2026-01-01","effective_to":""},
 {"id":"d2","name":"Loyalty","kind":"amount","value":"250.000000","compulsory":false,
  "all_customers":false,"customer_ids":["ACME-1"],"min_total":"5000.000000",
  "effective_from":"2026-11-01","effective_to":"2026-11-30"}]}
''';

const _empty = '{"discounts":[]}';

const _quote = '''
{"total":"1000.000000","discount":"100.000000","after":"900.000000",
 "lines":[{"name":"Trade terms","kind":"percent","value":"10.000000","amount":"100.000000"}]}
''';

/// A transport that answers the discount rules, and remembers what was sent to it.
class _Engine {
  _Engine({this.table = _twoRules, this.saveStatus = 200, this.saveBody});

  String table;
  int saveStatus;
  String? saveBody;
  final List<http.Request> sent = [];

  DiscountService get service {
    final built = DiscountService.fresh();
    built.useClient(
      MockClient((request) async {
        sent.add(request);
        final path = request.url.path;
        if (request.method == 'POST' && path.endsWith('/discounts/quote')) {
          return http.Response(_quote, 200);
        }
        if (request.method == 'POST' && path.endsWith('/discounts')) {
          final body =
              saveBody ??
              jsonEncode({
                'discount': {
                  'id': 'd3',
                  'name': 'Bulk',
                  'kind': 'percent',
                  'value': '5.000000',
                  'compulsory': true,
                  'all_customers': true,
                  'customer_ids': [],
                  'min_total': '5000.000000',
                  'effective_from': '2026-11-19',
                  'effective_to': '',
                },
              });
          return http.Response(body, saveStatus);
        }
        return http.Response(table, 200);
      }),
    );
    built.setAuthToken('token');
    return built;
  }
}

Future<void> _pump(WidgetTester tester, DiscountService service) async {
  await tester.pumpWidget(
    MaterialApp(
      home: DiscountsPage(service: service, today: '2026-11-19'),
    ),
  );
  await tester.pumpAndSettle();
}

/// The button sits at the bottom of a form that grows as choices are made, so it is brought into view first.
Future<void> _tapAddIt(WidgetTester tester) async {
  await tester.ensureVisible(find.text('Add it'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Add it'));
  await tester.pumpAndSettle();
}

Future<void> _addBulkDiscount(WidgetTester tester) async {
  await tester.tap(find.text('Add a discount'));
  await tester.pumpAndSettle();
  await tester.enterText(find.widgetWithText(TextField, 'Name'), 'Bulk');
  await tester.tap(find.text('A percentage'));
  await tester.pumpAndSettle();
  await tester.enterText(find.widgetWithText(TextField, 'Percentage'), '5');
  await tester.enterText(
    find.widgetWithText(TextField, 'Only once an invoice reaches'),
    '5000',
  );
  await _tapAddIt(tester);
}

void main() {
  testWidgets(
    'each rule says what it takes off, who it is for, and how it is given',
    (tester) async {
      await _pump(tester, _Engine().service);

      expect(find.text('Trade terms'), findsOneWidget);
      expect(find.text('10%'), findsOneWidget);
      expect(
        find.textContaining('takes 10% off every invoice for every customer'),
        findsOneWidget,
      );
      expect(
        find.textContaining('given automatically, from 2026-01-01'),
        findsOneWidget,
      );
      // a rule that names its customers and waits for a figure says so rather than looking like everyone's
      expect(
        find.textContaining(
          'takes 250 off invoices of 5000 and more for ACME-1',
        ),
        findsOneWidget,
      );
      expect(
        find.textContaining(
          'taken only when it is asked for, 2026-11-01 to 2026-11-30',
        ),
        findsOneWidget,
      );
    },
  );

  testWidgets(
    'a discount is sent with its kind, who it is for, and what it waits for',
    (tester) async {
      final engine = _Engine(table: _empty);
      await _pump(tester, engine.service);
      expect(find.text('This Book gives no discount yet'), findsOneWidget);

      await _addBulkDiscount(tester);

      final sent = engine.sent.lastWhere(
        (one) => one.method == 'POST' && one.url.path.endsWith('/discounts'),
      );
      final body = jsonDecode(sent.body) as Map<String, dynamic>;
      expect(body['name'], 'Bulk');
      expect(body['kind'], 'percent');
      expect(body['value'], '5');
      expect(body['compulsory'], isTrue);
      expect(body['all_customers'], isTrue);
      expect(body['customer_ids'], isEmpty);
      expect(body['min_total'], '5000');
      expect(body['effective_from'], '2026-11-19');
    },
  );

  testWidgets(
    'the figure shown after saving is the engine quote, not the app arithmetic',
    (tester) async {
      final engine = _Engine(table: _empty);
      await _pump(tester, engine.service);

      await _addBulkDiscount(tester);

      expect(
        find.textContaining('The customer pays 900.00'),
        findsOneWidget,
        reason: 'the answer shown is the engine\'s, asked for a thousand',
      );
    },
  );

  testWidgets(
    'a rule for named customers only is sent with them, and refuses to go without',
    (tester) async {
      final engine = _Engine(table: _empty);
      await _pump(tester, engine.service);

      await tester.tap(find.text('Add a discount'));
      await tester.pumpAndSettle();
      await tester.enterText(find.widgetWithText(TextField, 'Name'), 'Loyalty');
      await tester.enterText(find.widgetWithText(TextField, 'Percentage'), '5');
      await tester.tap(find.text('For every customer'));
      await tester.pumpAndSettle();

      // nobody named yet: the form says what is missing instead of sending a rule that could never be given
      await _tapAddIt(tester);
      expect(
        find.textContaining('Name the customers this discount is for'),
        findsOneWidget,
      );

      await tester.enterText(
        find.widgetWithText(TextField, 'Customers'),
        'ACME-1, OTHER-2',
      );
      await _tapAddIt(tester);

      final sent = engine.sent.lastWhere(
        (one) => one.method == 'POST' && one.url.path.endsWith('/discounts'),
      );
      final body = jsonDecode(sent.body) as Map<String, dynamic>;
      expect(body['all_customers'], isFalse);
      expect(body['customer_ids'], ['ACME-1', 'OTHER-2']);
    },
  );

  testWidgets('a refusal from the engine is shown in its own words', (
    tester,
  ) async {
    final engine = _Engine(
      table: _empty,
      saveStatus: 422,
      saveBody: jsonEncode({
        'error':
            'Bulk is for customer NOBODY-9, but this Book has no customer with that identifier. Add the customer '
            'first, or give the discount to all customers.',
      }),
    );
    await _pump(tester, engine.service);

    await _addBulkDiscount(tester);

    expect(find.text('This discount was not saved'), findsOneWidget);
    expect(
      find.textContaining('this Book has no customer with that identifier'),
      findsOneWidget,
    );
  });
}
