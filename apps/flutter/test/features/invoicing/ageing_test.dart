// How late the money is, worked out from the register.
//
// The band boundaries are the whole point of this screen, so they are tested at the edges rather than in the middle: a debt
// exactly 30 days late is one thing and 31 another, and an invoice with no due date is money not yet due rather than money
// that is late. Every figure comes from invoices the engine answered, so there is nothing here to keep in step.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/ageing_page.dart';

Invoice _invoice({
  required String number,
  required String customer,
  required String due,
  required String total,
  String paid = '0',
}) => Invoice(
  invoiceNumber: number,
  customerId: customer,
  invoiceDate: '2026-01-01',
  dueDate: due,
  total: ExactAmount.parse(total),
  paid: ExactAmount.parse(paid),
  status: 'issued',
  lines: const [],
);

void main() {
  final today = '2026-11-18';

  test('the band edges are exactly where they are said to be', () {
    final report = ageingReport([
      _invoice(
        number: 'A',
        customer: 'c1',
        due: '2026-11-18',
        total: '10',
      ), // due today
      _invoice(
        number: 'B',
        customer: 'c2',
        due: '2026-10-19',
        total: '20',
      ), // 30 days
      _invoice(
        number: 'C',
        customer: 'c3',
        due: '2026-10-18',
        total: '30',
      ), // 31 days
      _invoice(
        number: 'D',
        customer: 'c4',
        due: '2026-09-19',
        total: '40',
      ), // 60 days
      _invoice(
        number: 'E',
        customer: 'c5',
        due: '2026-09-18',
        total: '50',
      ), // 61 days
      _invoice(
        number: 'F',
        customer: 'c6',
        due: '2026-08-20',
        total: '60',
      ), // 90 days
      _invoice(
        number: 'G',
        customer: 'c7',
        due: '2026-08-19',
        total: '70',
      ), // 91 days
    ], today: today);

    expect(report.bands[0].amount.toDecimalString(), '10.00');
    expect(report.bands[1].amount.toDecimalString(), '20.00');
    expect(report.bands[2].amount.toDecimalString(), '70.00');
    expect(report.bands[3].amount.toDecimalString(), '110.00');
    expect(report.bands[4].amount.toDecimalString(), '70.00');
    expect(report.owed.toDecimalString(), '280.00');
    expect(report.late.toDecimalString(), '270.00');
  });

  test(
    'a settled invoice is not in it, and one with no due date is not late',
    () {
      final report = ageingReport([
        _invoice(
          number: 'A',
          customer: 'c1',
          due: '2026-01-01',
          total: '10',
          paid: '10',
        ),
        _invoice(number: 'B', customer: 'c2', due: '', total: '25'),
      ], today: today);

      expect(report.owed.toDecimalString(), '25.00');
      expect(report.late.isZero, isTrue);
      expect(report.bands[0].amount.toDecimalString(), '25.00');
      expect(report.rows.single.oldestDaysLate, 0);
    },
  );

  test('the worst late payer comes first, and the comfortable last', () {
    final report = ageingReport([
      _invoice(number: 'A', customer: 'fresh', due: '2026-12-01', total: '900'),
      _invoice(number: 'B', customer: 'middle', due: '2026-11-01', total: '50'),
      _invoice(number: 'C', customer: 'oldest', due: '2026-02-01', total: '20'),
    ], today: today);

    expect(report.rows.first.customerId, 'oldest');
    expect(report.rows[1].customerId, 'middle');
    expect(report.rows.last.customerId, 'fresh');
    expect(report.worst?.customerId, 'oldest');
    // the biggest debt is not the urgent one
    expect(report.rows.last.owed.toDecimalString(), '900.00');
  });

  testWidgets('the screen leads with what is late, not with the total', (
    tester,
  ) async {
    final service = InvoicingService.fresh()
      ..useClient(
        MockClient((request) async {
          if (request.url.path.endsWith('/invoices')) {
            return http.Response(
              jsonEncode([
                {
                  'invoice_number': 'INV-0001',
                  'customer_id': 'CUST-1',
                  'invoice_date': '2026-09-01',
                  'due_date': '2026-09-30',
                  'total_amount': '250.000000',
                  'paid_amount': '0',
                  'status': 'issued',
                  'items': const [],
                },
              ]),
              200,
            );
          }
          return http.Response(
            jsonEncode([
              {'id': 'c', 'customer_id': 'CUST-1', 'name': 'Moyo Traders'},
            ]),
            200,
          );
        }),
      );

    await tester.pumpWidget(
      MaterialApp(
        home: AgeingPage(service: service, today: '2026-11-18'),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('250.00 of 250.00 is past its due date'), findsOneWidget);
    expect(
      find.textContaining('Moyo Traders, 49 days past its date'),
      findsOneWidget,
    );
    expect(find.textContaining('61–90 days late'), findsNothing);
    expect(find.textContaining('31–60 days late'), findsOneWidget);
  });
}
