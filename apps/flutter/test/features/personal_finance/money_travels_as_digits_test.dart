// Money leaves these screens as its own digits. A figure past fifteen digits sent as a double is rewritten by the
// time it reaches the engine, and the tax estimate it drives is then wrong at the top of the range.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/personal_finance/data/personal_finance_service.dart';

void main() {
  late List<Map<String, dynamic>> sent;

  setUp(() {
    sent = [];
    PersonalFinanceService.instance.useClient(
      MockClient((request) async {
        if (request.body.isNotEmpty) {
          sent.add(jsonDecode(request.body) as Map<String, dynamic>);
        }
        return http.Response(jsonEncode({}), 200);
      }),
    );
  });

  test('a tax estimate carries every digit of the income', () async {
    const income = '123456789012345678901234567890.500000';
    await PersonalFinanceService.instance.estimateTax(
      annualIncome: ExactAmount.parse(income),
      deductions: ExactAmount.parse('1'),
      payePaid: ExactAmount.zero,
    );

    expect(sent, hasLength(1));
    expect(
      sent.single['annual_income'],
      income,
      reason: 'a double would have answered 1.2345678901234568e+29',
    );
    expect(sent.single['deductions'], '1.000000');
  });

  test('a debt payment carries every digit of the amount', () async {
    const amount = '999999999999999999999999999999.999999';
    await PersonalFinanceService.instance.payDebt(
      'd1',
      ExactAmount.parse(amount),
    );
    expect(sent.single['amount'], amount);
  });

  test(
    'a share price travels as digits, with the units left a count',
    () async {
      await PersonalFinanceService.instance.trade(
        'i1',
        side: 'buy',
        units: 10,
        price: ExactAmount.parse('0.000001'),
      );
      expect(sent.single['price'], '0.000001');
      expect(sent.single['units'], 10);
    },
  );
}
