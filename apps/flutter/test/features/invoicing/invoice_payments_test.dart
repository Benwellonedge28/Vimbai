// Payments against an invoice: what is sent, what is refused before it is sent, and what the screen promises.
//
// Two things must hold. An overpayment is refused on the phone, because the engine would faithfully add it and a customer
// would owe a negative amount nobody can explain. And a settled invoice must not read as if the money had reached the
// ledger: recording a payment is bookkeeping against the invoice, and the journal entry is somebody's separate act.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/invoice_detail_page.dart';

Invoice _invoice() => Invoice(
  invoiceNumber: 'INV-0001',
  customerId: 'acme',
  invoiceDate: '2026-11-18',
  dueDate: '2026-12-18',
  total: ExactAmount.parse('779.985'),
  paid: ExactAmount.parse('100'),
  status: 'outstanding',
  lines: [
    InvoiceLine(
      description: 'Consulting',
      quantity: ExactAmount.parse('3'),
      unitPrice: ExactAmount.parse('250.00'),
    ),
  ],
);

void main() {
  setUp(() => SessionToken.set('a-session-token'));
  tearDown(() => SessionToken.clear());

  test('a payment is sent as exact text against the invoice number', () async {
    late Uri asked;
    late Map<String, dynamic> sent;
    final service = InvoicingService.fresh()
      ..useClient(
        MockClient((request) async {
          asked = request.url;
          sent = jsonDecode(request.body) as Map<String, dynamic>;
          return http.Response(
            jsonEncode({
              'invoice_number': 'INV-0001',
              'payment_amount': '679.985000',
              'paid_amount': '779.985000',
              'outstanding': '0.000000',
              'status': 'paid',
              'journal_posted': false,
            }),
            200,
          );
        }),
      );

    final result = await service.recordPayment(
      _invoice(),
      ExactAmount.parse('679.985'),
      on: DateTime(2026, 11, 20),
    );

    expect(asked.path, endsWith('/invoices/INV-0001/record-payment'));
    expect(sent['payment_amount'], '679.985000');
    expect(sent['payment_date'], '2026-11-20');
    expect(result.settled, isTrue);
    expect(
      result.status,
      'paid',
      reason:
          'the engine said paid; the phone must not invent a different state',
    );
    expect(
      result.journalPosted,
      isFalse,
      reason: 'the engine wrote no ledger entry, so nothing here may claim one',
    );
  });

  test('more than is owing is refused before anything is sent', () async {
    var sent = 0;
    final service = InvoicingService.fresh()
      ..useClient(
        MockClient((_) async {
          sent++;
          return http.Response('{}', 200);
        }),
      );

    await expectLater(
      service.recordPayment(_invoice(), ExactAmount.parse('680')),
      throwsA(
        predicate(
          (failure) => failure.toString().contains('still owing on INV-0001'),
        ),
      ),
    );
    expect(
      sent,
      0,
      reason: 'a refusal the phone can make is not the engine\'s to answer',
    );
  });

  testWidgets(
    'the detail screen shows what is owing and settles in its own words',
    (tester) async {
      final service = InvoicingService.fresh()
        ..useClient(
          MockClient((request) async {
            return http.Response(
              jsonEncode({
                'invoice_number': 'INV-0001',
                'payment_amount': '679.985000',
                'paid_amount': '779.985000',
                'outstanding': '0.000000',
                'status': 'paid',
              }),
              200,
            );
          }),
        );

      await tester.pumpWidget(
        MaterialApp(
          home: InvoiceDetailPage(invoice: _invoice(), service: service),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('679.985 still to come in'), findsOneWidget);
      expect(find.textContaining('Consulting'), findsOneWidget);

      await tester.tap(find.text('Record a payment'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Record it'));
      await tester.pumpAndSettle();

      expect(find.textContaining('is settled'), findsOneWidget);
      expect(find.textContaining('posts in Journal entries'), findsOneWidget);
      expect(find.text('Record a payment'), findsNothing);
    },
  );
}
