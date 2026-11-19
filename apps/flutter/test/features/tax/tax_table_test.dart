// The taxes a Book charges, and the screen where a new one is added.
//
// The point of this screen is that no tax type is built into the app: adding one is a row. So what is pinned here is
// that a tax the owner adds is sent with its own rate, account and scope, that the answer shown afterwards is the
// engine's rather than the app's own arithmetic, and that a refusal is repeated in the engine's words.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/tax/data/tax_table.dart';
import 'package:vimbai_mobile_client/features/tax/presentation/tax_table_page.dart';

const _vatOnly = '''
{"date":"2026-11-19","count":2,"taxes":[
 {"id":"t1","name":"VAT","rate":"15.5000","scope":"both","collected_account":"2100",
  "reclaim_account":"2100","applies_after":[],"effective_from":"2026-01-01","effective_to":"",
  "in_force":true},
 {"id":"t2","name":"Levy","rate":"2.0000","scope":"sales","collected_account":"2100",
  "reclaim_account":"","applies_after":["VAT"],"effective_from":"2027-01-01","effective_to":"",
  "in_force":false}]}
''';

const _seeded = '''
{"date":"2026-11-19","count":1,"taxes":[
 {"id":"t1","name":"VAT","rate":"15.5000","scope":"both","collected_account":"2100",
  "reclaim_account":"2100","applies_after":[],"effective_from":"2026-01-01","effective_to":"",
  "in_force":true}]}
''';

const _empty = '{"date":"2026-11-19","count":0,"taxes":[]}';

const _preview = '''
{"date":"2026-11-19","direction":"sale","net":"845.000000","tax":"155.000000",
 "gross":"1000.000000",
 "tax_lines":[{"tax":"VAT","rate":"15.5000","base":"845.000000","amount":"155.000000",
               "side":"credit","account_number":"2100","expense_account":"6900"}],
 "note":""}
''';

/// A transport that answers the tax table, and remembers what was sent to it.
class _Engine {
  _Engine({this.table = _vatOnly, this.saveStatus = 200, this.saveBody});

  String table;
  int saveStatus;
  String? saveBody;
  final List<http.Request> sent = [];

  TaxTableService get service {
    final built = TaxTableService.fresh();
    built.useClient(
      MockClient((request) async {
        sent.add(request);
        final path = request.url.path;
        if (request.method == 'POST' && path.endsWith('/taxes/seed')) {
          table = _seeded;
          return http.Response('{"status":"seeded"}', 200);
        }
        if (request.method == 'POST' && path.endsWith('/taxes/compute')) {
          return http.Response(_preview, 200);
        }
        if (request.method == 'POST' && path.endsWith('/taxes')) {
          final body =
              saveBody ??
              jsonEncode({
                'status': 'saved',
                'tax': {
                  'id': 't3',
                  'name': 'VAT',
                  'rate': '15.5000',
                  'scope': 'both',
                  'collected_account': '2100',
                  'reclaim_account': '2100',
                  'applies_after': [],
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

Future<void> _pump(WidgetTester tester, TaxTableService service) async {
  await tester.pumpWidget(
    MaterialApp(
      home: TaxTablePage(service: service, today: '2026-11-19'),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('the table says what each tax takes and when it holds', (
    tester,
  ) async {
    await _pump(tester, _Engine().service);

    expect(find.text('VAT'), findsOneWidget);
    expect(find.text('15.5%'), findsOneWidget);
    expect(
      find.textContaining(
        'charges 15.5% on sales and purchases from 2026-01-01',
      ),
      findsOneWidget,
    );
    // a tax defined but not yet in force is listed, and says so rather than looking live
    expect(find.text('Levy'), findsOneWidget);
    expect(find.text('Not in force on this day.'), findsOneWidget);
    expect(
      find.textContaining('One tax applies on 2026-11-19'),
      findsOneWidget,
    );
  });

  testWidgets('a Book with no taxes offers the usual ones', (tester) async {
    final engine = _Engine(table: _empty);
    await _pump(tester, engine.service);

    expect(find.text('This Book charges no tax yet'), findsOneWidget);
    expect(find.text('Fill in the usual taxes'), findsOneWidget);

    await tester.tap(find.text('Fill in the usual taxes'));
    await tester.pumpAndSettle();

    expect(
      engine.sent.any(
        (r) => r.method == 'POST' && r.url.path.endsWith('/taxes/seed'),
      ),
      isTrue,
    );
    expect(find.text('VAT'), findsOneWidget);
  });

  testWidgets('a tax added on the phone is sent with its own rate and account', (
    tester,
  ) async {
    final engine = _Engine();
    await _pump(tester, engine.service);

    await tester.tap(find.text('Add a tax'));
    await tester.pumpAndSettle();

    await tester.enterText(
      find.widgetWithText(TextField, 'What it is called'),
      'VAT',
    );
    await tester.enterText(
      find.widgetWithText(TextField, 'Percentage'),
      '15.5',
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Save this tax'));
    await tester.pumpAndSettle();

    final saved = engine.sent.firstWhere(
      (r) => r.method == 'POST' && r.url.path.endsWith('/taxes'),
    );
    final body = jsonDecode(saved.body) as Map<String, dynamic>;
    expect(body['name'], 'VAT');
    expect(body['rate'], '15.5');
    expect(body['scope'], 'both');
    expect(body['collected_account'], '2100');
    expect(body['effective_from'], '2026-11-19');
    expect(body['applies_after'], isEmpty);

    // the figure shown afterwards is the engine's answer, not one the app worked out
    expect(
      find.text(
        'The sale records 1000.00 and the tax is 155.00 in expenses: VAT 155.00 to account 2100.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('a tax with no account is not sent at all', (tester) async {
    final engine = _Engine(table: _empty);
    await _pump(tester, engine.service);

    await tester.tap(find.text('Add a tax'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'What it is called'),
      'Stamp',
    );
    await tester.enterText(find.widgetWithText(TextField, 'Percentage'), '1');
    await tester.pumpAndSettle();

    expect(
      find.text('Say which account it posts to, or it cannot be charged.'),
      findsOneWidget,
    );
    final save = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Save this tax'),
    );
    expect(save.onPressed, isNull);
    expect(engine.sent.any((r) => r.method == 'POST'), isFalse);
  });

  testWidgets('the engine refusal is repeated in the engine words', (
    tester,
  ) async {
    final engine = _Engine(
      saveStatus: 422,
      saveBody: jsonEncode({
        'error':
            'VAT does not apply on 2025-06-01: this Book has it from 2026-01-01 to now.',
      }),
    );
    await _pump(tester, engine.service);

    await tester.tap(find.text('Add a tax'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'What it is called'),
      'VAT',
    );
    await tester.enterText(
      find.widgetWithText(TextField, 'Percentage'),
      '15.5',
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Save this tax'));
    await tester.pumpAndSettle();

    expect(find.text('This tax was not saved'), findsOneWidget);
    expect(
      find.text(
        'VAT does not apply on 2025-06-01: this Book has it from 2026-01-01 to now.',
      ),
      findsOneWidget,
    );
  });

  test('a rate is read back without the zeros that keep it exact', () {
    final tax = BookTax.fromJson(const {
      'name': 'VAT',
      'rate': '15.5000',
      'scope': 'both',
      'collected_account': '2100',
      'effective_from': '2026-01-01',
    });
    expect(tax.rateText, '15.5%');
    expect(tax.problem, isEmpty);
    expect(tax.onSales && tax.onPurchases, isTrue);

    final nowhere = BookTax.fromJson(const {
      'name': 'Stamp',
      'rate': '1.0000',
      'scope': 'sales',
      'effective_from': '2026-01-01',
    });
    expect(nowhere.problem, contains('has no account'));
  });
}
