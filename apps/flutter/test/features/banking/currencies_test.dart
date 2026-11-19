// The currencies a Book keeps, and the screen where it chooses them from the world's list.
//
// What is pinned here is that the app never decides how many decimal places a currency has: it reads the catalogue
// from the engine, offers those currencies rather than a typed code, and adds one by sending its code alone.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/banking/data/currencies.dart';
import 'package:vimbai_mobile_client/features/banking/presentation/currencies_page.dart';

const _catalogue = r'''
{"count":3,"currencies":[
 {"code":"JPY","name":"Japanese Yen","decimal_places":0},
 {"code":"KWD","name":"Kuwaiti Dinar","decimal_places":3},
 {"code":"USD","name":"US Dollar","decimal_places":2}]}
''';

const _held = r'''
{"currencies":[
 {"code":"USD","name":"US Dollar","symbol":"US$","decimal_places":2,"is_active":true}]}
''';

const _heldWithDinar = r'''
{"currencies":[
 {"code":"USD","name":"US Dollar","symbol":"US$","decimal_places":2,"is_active":true},
 {"code":"KWD","name":"Kuwaiti Dinar","symbol":"KD","decimal_places":3,"is_active":true}]}
''';

class _Engine {
  _Engine({this.addStatus = 200, this.addBody});

  int addStatus;
  String? addBody;
  String held = _held;
  final List<http.Request> sent = [];

  CurrencyService get service {
    final built = CurrencyService.fresh();
    built.useClient(
      MockClient((request) async {
        sent.add(request);
        final path = request.url.path;
        if (path.endsWith('/currencies/catalogue')) {
          return http.Response(_catalogue, 200);
        }
        if (request.method == 'POST' && path.endsWith('/currencies')) {
          if (addStatus != 200) {
            return http.Response(addBody ?? '{}', addStatus);
          }
          held = _heldWithDinar;
          return http.Response('{"code":"KWD"}', 201);
        }
        return http.Response(held, 200);
      }),
    );
    return built;
  }
}

Future<void> _pump(WidgetTester tester, CurrencyService service) async {
  await tester.pumpWidget(
    MaterialApp(
      home: CurrenciesPage(service: service, today: '2026-11-19'),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'each currency the Book keeps says how many places its money has',
    (tester) async {
      final engine = _Engine();
      await _pump(tester, engine.service);

      expect(find.text('USD'), findsOneWidget);
      expect(
        find.text('US Dollar (USD), two decimal places.'),
        findsOneWidget,
        reason:
            'the places are the engine\'s answer, not the app\'s assumption',
      );
    },
  );

  testWidgets('the world is offered, and the one picked is sent by its code', (
    tester,
  ) async {
    final engine = _Engine();
    await _pump(tester, engine.service);

    await tester.tap(find.text('Add a currency'));
    await tester.pumpAndSettle();

    // the Book's own currency is not offered again
    expect(find.text('USD · US Dollar'), findsNothing);
    expect(find.text('KWD · Kuwaiti Dinar'), findsOneWidget);
    expect(find.text('three decimal places'), findsOneWidget);

    await tester.tap(find.text('KWD · Kuwaiti Dinar'));
    await tester.pumpAndSettle();

    final posted = engine.sent.lastWhere(
      (one) => one.method == 'POST' && one.url.path.endsWith('/currencies'),
    );
    final body = jsonDecode(posted.body) as Map<String, dynamic>;
    expect(
      body['code'],
      'KWD',
      reason: 'the code is all the engine needs: it knows the places',
    );
    expect(body.containsKey('decimal_places'), isFalse);
    expect(find.textContaining('Kuwaiti Dinar added'), findsOneWidget);
    expect(
      find.text('Kuwaiti Dinar (KWD), three decimal places.'),
      findsOneWidget,
    );
  });

  testWidgets('a refusal from the engine is shown in its own words', (
    tester,
  ) async {
    final engine = _Engine(
      addStatus: 422,
      addBody: '{"detail":"KWD is not a currency the world uses."}',
    );
    await _pump(tester, engine.service);

    await tester.tap(find.text('Add a currency'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('KWD · Kuwaiti Dinar'));
    await tester.pumpAndSettle();

    expect(find.text('KWD is not a currency the world uses.'), findsOneWidget);
  });
}
