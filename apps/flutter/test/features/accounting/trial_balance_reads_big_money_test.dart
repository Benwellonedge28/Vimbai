// A trial balance has to be readable at the size the ledger actually allows.
//
// `vimbai_common::Money` has no ceiling on whole digits, so the engine can answer with a thirty-digit total. The phone
// used to read every statement figure into a `double`, which holds fifteen significant digits, and print it with
// toStringAsFixed(2): a balance of 1,234,567,890,123,456,789,012,345,678,901.00 came back as something else entirely, and
// the two sides then looked unequal when they were not. The statement figures are now held as digits and printed as they
// arrived.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/balance_sheet_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/trial_balance_page.dart';

void main() {
  final originalTransport = ApiClient.transport;

  setUp(() => SharedPreferences.setMockInitialValues({}));
  tearDown(() => ApiClient.transport = originalTransport);

  const big = '1234567890123456789012345678901.250000';

  testWidgets('a thirty-digit balance is shown as the engine wrote it', (
    tester,
  ) async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode({
          'report_date': '2026-11-17T00:00:00Z',
          'accounts': [
            {
              'account_number': '1000',
              'account_name': 'Cash',
              'account_type': 'asset',
              'debit': big,
              'credit': '0.000000',
            },
            {
              'account_number': '4000',
              'account_name': 'Sales',
              'account_type': 'revenue',
              'debit': '0.000000',
              'credit': big,
            },
          ],
          'total_debits': big,
          'total_credits': big,
          'is_balanced': true,
        }),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    await tester.pumpWidget(const MaterialApp(home: TrialBalancePage()));
    // the screen shows a spinner while the engine is asked, so settle on the answer rather than on the animation
    await tester.pump();
    await tester.pump(const Duration(seconds: 2));

    // every digit, in the order it arrived
    expect(find.text('1234567890123456789012345678901.25'), findsWidgets);
    // and the sides agree, because the digits agree - not because two doubles rounded to the same value
    expect(find.text('Trial Balance is Balanced!'), findsOneWidget);
  });

  testWidgets('a balance sheet shows a thirty-digit total in full', (
    tester,
  ) async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode({
          'as_of_date': '2026-11-17T00:00:00Z',
          'assets': [
            {'category': 'Cash', 'amount': big},
          ],
          'liabilities': <dynamic>[],
          'equity': <dynamic>[],
          'total_assets': big,
          'total_liabilities_equity': big,
        }),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    await tester.pumpWidget(const MaterialApp(home: BalanceSheetPage()));
    await tester.pump();
    await tester.pump(const Duration(seconds: 2));

    // every digit, in full, and no currency symbol: the Book's currency is not the app's to assume
    expect(find.text('1234567890123456789012345678901.25'), findsWidgets);
  });
}
