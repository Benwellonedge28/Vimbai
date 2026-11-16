// What the personal finance screens read back from the engine, proven against a transport rather than a server.
//
// The engine answers debts, recurring items and holdings as bare arrays. The service decoded every reply as an
// object, so the lists threw before a row reached the page. Both shapes are held here so neither can drift away,
// and a debt opened with the phone's own payload is recognised as opened.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/personal_finance/data/personal_finance_service.dart';

void main() {
  test('debts read from a bare array, which is what the engine answers', () async {
    PersonalFinanceService.instance.useClient(
      MockClient((request) async {
        return http.Response(
          jsonEncode([
            {'id': 'd1', 'name': 'Car loan', 'principal': '1000.00'},
          ]),
          200,
        );
      }),
    );

    final debts = await PersonalFinanceService.instance.listDebts();

    expect(debts, hasLength(1));
    expect(debts.single['name'], 'Car loan');
  });

  test('debts read from a wrapped answer too', () async {
    PersonalFinanceService.instance.useClient(
      MockClient((request) async {
        return http.Response(
          jsonEncode({
            'debts': [
              {'id': 'd1', 'name': 'Car loan'},
            ],
          }),
          200,
        );
      }),
    );

    final debts = await PersonalFinanceService.instance.listDebts();

    expect(debts.single['name'], 'Car loan');
  });

  test('recurring items read from a bare array', () async {
    PersonalFinanceService.instance.useClient(
      MockClient((request) async {
        return http.Response(
          jsonEncode([
            {'id': 'r1', 'description': 'rent'},
          ]),
          200,
        );
      }),
    );

    final rows = await PersonalFinanceService.instance.listRecurring();

    expect(rows.single['description'], 'rent');
  });

  test('holdings are found under the name the page reads them by', () async {
    PersonalFinanceService.instance.useClient(
      MockClient((request) async {
        expect(request.url.path, endsWith('/investments'));
        return http.Response(
          jsonEncode([
            {'id': 'i1', 'name': 'CBZ shares'},
          ]),
          200,
        );
      }),
    );

    final portfolio = await PersonalFinanceService.instance.portfolio();

    expect((portfolio['investments'] as List).single['name'], 'CBZ shares');
  });

  test('a debt is opened with the payload the phone posts', () async {
    late http.Request asked;
    PersonalFinanceService.instance.useClient(
      MockClient((request) async {
        asked = request;
        return http.Response(jsonEncode({'id': 'd1', 'name': 'Car loan'}), 201);
      }),
    );

    await PersonalFinanceService.instance.createDebt(
      name: 'Car loan',
      principal: 1000,
      annualRate: 12,
      termMonths: 12,
      startedAt: '2026-01-01',
    );

    final body = jsonDecode(asked.body) as Map<String, dynamic>;
    expect(body['term_months'], 12);
    expect(body['annual_rate'], 12);
    expect(asked.url.path, endsWith('/debts'));
  });
}
