// A discount taken from the invoice screen.
//
// The rules are the Book's, so the form reads them and offers the ones that are only given when asked for, shows what
// the engine says would come off the figure on screen, and sends the names it took. A compulsory rule is never sent:
// the engine gives it without being asked, and sending it again would look like a second one.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/discounts.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/invoices_page.dart';

const _customers = '''
[{"id":"c1","customer_id":"ACME-1","name":"Acme","email":"acme@example.com"}]
''';

const _noInvoices = '[]';

const _rules = '''
{"discounts":[
 {"id":"d1","name":"Trade terms","kind":"percent","value":"10.000000","compulsory":true,
  "all_customers":true,"customer_ids":[],"min_total":"0.000000",
  "effective_from":"2026-01-01","effective_to":""},
 {"id":"d2","name":"Loyalty","kind":"amount","value":"250.000000","compulsory":false,
  "all_customers":true,"customer_ids":[],"min_total":"0.000000",
  "effective_from":"2026-01-01","effective_to":""}]}
''';

const _quote = '''
{"total":"1000.000000","discount":"250.000000","after":"750.000000",
 "lines":[{"name":"Loyalty","kind":"amount","value":"250.000000","amount":"250.000000"}]}
''';

const _raised = '''
{"invoice_number":"INV-0001","customer_id":"ACME-1","invoice_date":"2026-11-19",
 "due_date":"2026-12-19","total_amount":"1000.000000","discount_amount":"350.000000",
 "amount_due":"650.000000","paid":"0","status":"outstanding",
 "items":[{"description":"Car","quantity":"1","unit_price":"1000.00"}]}
''';

class _Engine {
  final List<http.Request> sent = [];

  http.Client get client => MockClient((request) async {
    sent.add(request);
    final path = request.url.path;
    if (path.endsWith('/discounts/quote')) return http.Response(_quote, 200);
    if (path.endsWith('/discounts')) return http.Response(_rules, 200);
    if (request.method == 'POST' && path.endsWith('/invoices')) {
      return http.Response(_raised, 201);
    }
    if (path.endsWith('/customers')) return http.Response(_customers, 200);
    return http.Response(_noInvoices, 200);
  });

  InvoicingService get invoices => InvoicingService.fresh()..useClient(client);

  DiscountService get discounts => DiscountService.fresh()..useClient(client);
}

Future<void> _openTheForm(WidgetTester tester, _Engine engine) async {
  await tester.pumpWidget(
    MaterialApp(
      home: InvoicesPage(
        service: engine.invoices,
        discountService: engine.discounts,
      ),
    ),
  );
  await tester.pumpAndSettle();
  await tester.tap(find.textContaining('Raise'));
  await tester.pumpAndSettle();
}

/// The button sits at the foot of a form that grows, so it is brought into view before it is pressed.
Future<void> _raiseIt(WidgetTester tester) async {
  await tester.ensureVisible(find.text('Raise it'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Raise it'));
  await tester.pumpAndSettle();
}

Future<void> _fillOneLine(WidgetTester tester) async {
  await tester.enterText(
    find.widgetWithText(TextField, 'Invoice number'),
    'INV-0001',
  );
  await tester.enterText(
    find.widgetWithText(TextField, 'What was sold'),
    'Car',
  );
  await tester.enterText(find.widgetWithText(TextField, 'Quantity'), '1');
  await tester.enterText(find.widgetWithText(TextField, 'Unit price'), '1000');
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('the form offers the rules that are only given when asked for', (
    tester,
  ) async {
    final engine = _Engine();
    await _openTheForm(tester, engine);

    expect(find.text('Discount'), findsOneWidget);
    // the compulsory rule is stated, not offered: the engine gives it without being asked
    expect(
      find.textContaining('The engine gives this without being asked'),
      findsOneWidget,
    );
    expect(find.widgetWithText(CheckboxListTile, 'Loyalty'), findsOneWidget);
    expect(find.widgetWithText(CheckboxListTile, 'Trade terms'), findsNothing);
  });

  testWidgets(
    'taking a rule shows the engine figure and sends the rule by name',
    (tester) async {
      final engine = _Engine();
      await _openTheForm(tester, engine);
      await _fillOneLine(tester);

      // the tile sits below the lines, so it is brought into view before it is ticked
      await tester.ensureVisible(find.text('Loyalty'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Loyalty'));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('The customer pays 750.00'),
        findsOneWidget,
        reason:
            'the figure shown is the engine quote for the invoice on screen',
      );

      await _raiseIt(tester);

      final sent = engine.sent.lastWhere(
        (one) => one.method == 'POST' && one.url.path.endsWith('/invoices'),
      );
      final body = jsonDecode(sent.body) as Map<String, dynamic>;
      expect(body['discount_rules'], ['Loyalty']);
      expect(
        body.containsKey('discount_amount'),
        isFalse,
        reason: 'nothing was typed into the one-off box',
      );
    },
  );

  testWidgets(
    'a figure given by hand goes with the invoice as the engine reads it',
    (tester) async {
      final engine = _Engine();
      await _openTheForm(tester, engine);
      await _fillOneLine(tester);

      await tester.enterText(
        find.widgetWithText(TextField, 'A one-off discount'),
        '100',
      );
      await tester.pumpAndSettle();
      await _raiseIt(tester);

      final sent = engine.sent.lastWhere(
        (one) => one.method == 'POST' && one.url.path.endsWith('/invoices'),
      );
      final body = jsonDecode(sent.body) as Map<String, dynamic>;
      expect(body['discount_amount'], '100');
      expect(body.containsKey('discount_rules'), isFalse);
    },
  );
}
