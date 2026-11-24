// What the business still holds in each money.
//
// The screen exists to answer two questions at once without ever mixing them: how much of another money is still coming, and
// how much is still going. A hundred dollars owed to the business and a hundred dollars owed by it are both a hundred
// dollars, but a page that netted them would be describing neither.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/accounting/data/currency_exposure.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/currency_exposure_page.dart';

/// A transport that answers the balances route with one Book's holdings.
CurrencyExposureService _answering(
  Map<String, dynamic> body, {
  int status = 200,
}) {
  final service = CurrencyExposureService.fresh();
  service.useClient(
    MockClient((request) async {
      expect(request.url.path, endsWith('/accounting/fx-balances'));
      return http.Response(jsonEncode(body), status);
    }),
  );
  return service;
}

void main() {
  testWidgets(
    'each money stands on its own, with what is coming and what is going',
    (tester) async {
      final service = _answering({
        'currencies': [
          {
            'currency': 'USD',
            'units': '1000.00',
            'carried': '25500.00',
            'documents': [
              {
                'reference': 'INV-9',
                'kind': 'sale',
                'date': '2026-11-02',
                'rate': '25.500000',
                'units': '1000.00',
                'carried': '25500.00',
                'outstanding_units': '1000.00',
                'outstanding_carried': '25500.00',
              },
              {
                'reference': 'BILL-4',
                'kind': 'bill',
                'date': '2026-11-05',
                'rate': '26.000000',
                'units': '400.00',
                'carried': '10400.00',
                'outstanding_units': '400.00',
                'outstanding_carried': '10400.00',
              },
            ],
          },
          {
            'currency': 'GBP',
            'units': '50.00',
            'carried': '68.00',
            'documents': [
              {
                'reference': 'INV-11',
                'kind': 'sale',
                'date': '2026-11-09',
                'rate': '1.360000',
                'units': '50.00',
                'carried': '68.00',
                'outstanding_units': '50.00',
                'outstanding_carried': '68.00',
              },
            ],
          },
        ],
        'unmatched_settlements': [],
      });

      await tester.pumpWidget(
        MaterialApp(home: CurrencyExposurePage(service: service)),
      );
      await tester.pumpAndSettle();

      // the two directions are named, and the money is named with the figure
      expect(find.text('Owed to you'), findsNWidgets(2));
      expect(find.text('You owe'), findsOneWidget);
      expect(find.text('1000.00 USD'), findsOneWidget);
      expect(find.text('400.00 USD'), findsOneWidget);
      expect(find.text('50.00 GBP'), findsOneWidget);

      // the document behind each figure can be followed, with its rate
      expect(find.textContaining('INV-9'), findsOneWidget);
      expect(find.textContaining('at 25.50'), findsOneWidget);
      expect(find.textContaining('BILL-4'), findsOneWidget);
    },
  );

  testWidgets(
    'a money whose documents are all paid says so rather than showing nothing',
    (tester) async {
      final service = _answering({
        'currencies': [
          {
            'currency': 'USD',
            'units': '0',
            'carried': '0',
            'documents': [
              {
                'reference': 'INV-9',
                'kind': 'sale',
                'date': '2026-11-02',
                'rate': '25.500000',
                'units': '1000.00',
                'carried': '25500.00',
                'outstanding_units': '0',
                'outstanding_carried': '0',
              },
            ],
          },
        ],
        'unmatched_settlements': [],
      });

      await tester.pumpWidget(
        MaterialApp(home: CurrencyExposurePage(service: service)),
      );
      await tester.pumpAndSettle();

      expect(
        find.textContaining('Nothing is still owed either way'),
        findsOneWidget,
      );
      expect(find.text('Owed to you'), findsNothing);
    },
  );

  testWidgets(
    'a payment that named no document is listed, not spread over other bills',
    (tester) async {
      final service = _answering({
        'currencies': const [],
        'unmatched_settlements': [
          {'currency': 'USD', 'document': 'PAY-77', 'units': '200.00'},
        ],
      });

      await tester.pumpWidget(
        MaterialApp(home: CurrencyExposurePage(service: service)),
      );
      await tester.pumpAndSettle();

      expect(find.text('PAY-77'), findsOneWidget);
      expect(find.textContaining('200.00 USD'), findsOneWidget);
      expect(
        find.textContaining('not been set against anything'),
        findsOneWidget,
      );
    },
  );

  testWidgets('a Book that has never traded abroad says so', (tester) async {
    final service = _answering({
      'currencies': const [],
      'unmatched_settlements': const [],
    });

    await tester.pumpWidget(
      MaterialApp(home: CurrencyExposurePage(service: service)),
    );
    await tester.pumpAndSettle();

    expect(find.text('Everything is in your own money'), findsOneWidget);
  });

  testWidgets('a refused reading is shown in the engine\'s own words', (
    tester,
  ) async {
    final service = _answering({
      'detail': 'This Book has no reporting currency set.',
    }, status: 400);

    await tester.pumpWidget(
      MaterialApp(home: CurrencyExposurePage(service: service)),
    );
    await tester.pumpAndSettle();

    expect(
      find.textContaining('This Book has no reporting currency set.'),
      findsOneWidget,
    );
  });
}
