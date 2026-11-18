// A customer statement adds up.
//
// The one thing a statement must never do is disagree with the Book it came from, so these tests hold it to that: the closing
// figure is the register's own outstanding total for that customer, reached by adding up the same events in date order. A
// part payment moves the balance without removing the invoice, another customer's money never appears, and a payment with no
// readable date is still counted rather than quietly dropped.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/customer_statement_page.dart';

Invoice _invoice({
  required String number,
  required String customer,
  required String raised,
  required String due,
  required String total,
  String paid = '0',
}) => Invoice(
  invoiceNumber: number,
  customerId: customer,
  invoiceDate: raised,
  dueDate: due,
  total: ExactAmount.parse(total),
  paid: ExactAmount.parse(paid),
  status: 'issued',
  lines: const [],
);

InvoicePaymentEntry _payment(String date, String amount) =>
    InvoicePaymentEntry(date: date, amount: ExactAmount.parse(amount));

void main() {
  const today = '2026-11-18';

  test('the events land in date order and the balance climbs and falls', () {
    final statement = customerStatement(
      [
        _invoice(
          number: 'INV-0001',
          customer: 'acme',
          raised: '2026-09-01',
          due: '2026-09-30',
          total: '250',
          paid: '100',
        ),
        _invoice(
          number: 'INV-0002',
          customer: 'acme',
          raised: '2026-11-01',
          due: '2026-12-01',
          total: '455',
        ),
        _invoice(
          number: 'INV-0003',
          customer: 'beta',
          raised: '2026-09-05',
          due: '2026-10-05',
          total: '999',
        ),
      ],
      {
        'INV-0001': [_payment('2026-09-20', '100')],
        'INV-0003': [_payment('2026-09-10', '999')],
      },
      customerId: 'acme',
      today: today,
      name: 'Acme Ltd',
    );

    expect(statement.lines.map((line) => line.date), [
      '2026-09-01',
      '2026-09-20',
      '2026-11-01',
    ]);
    expect(statement.lines[0].running.toDecimalString(), '250.00');
    expect(statement.lines[1].running.toDecimalString(), '150.00');
    expect(statement.lines[2].running.toDecimalString(), '605.00');
    // the register's own answer for the same customer, reached the other way round
    expect(statement.closing.toDecimalString(), '605.00');
    expect(statement.charged.toDecimalString(), '705.00');
    expect(statement.paid.toDecimalString(), '100.00');
    // another customer's invoice and payment are not on it
    expect(statement.lines.length, 3);
  });

  test('a settled invoice stays on the statement and closes at nothing', () {
    final statement = customerStatement(
      [
        _invoice(
          number: 'INV-0001',
          customer: 'acme',
          raised: '2026-09-01',
          due: '2026-09-30',
          total: '250',
          paid: '250',
        ),
      ],
      {
        'INV-0001': [_payment('2026-09-30', '250')],
      },
      customerId: 'acme',
      today: today,
      name: 'Acme Ltd',
    );

    expect(statement.lines.length, 2);
    expect(statement.closing.isZero, isTrue);
    expect(statement.late.isZero, isTrue);
  });

  test('a payment that lost its date is still money received', () {
    final statement = customerStatement(
      [
        _invoice(
          number: 'INV-0001',
          customer: 'acme',
          raised: '2026-09-01',
          due: '2026-09-30',
          total: '250',
          paid: '50',
        ),
      ],
      {
        'INV-0001': [_payment('', '50')],
      },
      customerId: 'acme',
      today: today,
    );

    expect(statement.paid.toDecimalString(), '50.00');
    expect(statement.closing.toDecimalString(), '200.00');
  });

  test('only money past its date is called late', () {
    final statement = customerStatement(
      [
        _invoice(
          number: 'INV-0001',
          customer: 'acme',
          raised: '2026-09-01',
          due: '2026-09-01',
          total: '250',
          paid: '150',
        ),
        _invoice(
          number: 'INV-0002',
          customer: 'acme',
          raised: '2026-11-10',
          due: '2026-12-10',
          total: '455',
        ),
      ],
      {
        'INV-0001': [_payment('2026-11-01', '150')],
      },
      customerId: 'acme',
      today: today,
    );

    // only what is still owing on the overdue invoice, not the whole invoice
    expect(statement.late.toDecimalString(), '100.00');
    expect(statement.oldestDaysLate, 78);
    expect(statement.closing.toDecimalString(), '555.00');
  });

  testWidgets(
    'the page prints the closing figure and the movements that reach it',
    (tester) async {
      final service = InvoicingService.fresh()
        ..useClient(
          MockClient((request) async {
            if (request.url.path.endsWith('/payments')) {
              return http.Response(
                jsonEncode([
                  {
                    'amount': '100.000000',
                    'payment_date': '2026-09-20',
                    'invoice_number': 'INV-0001',
                  },
                ]),
                200,
              );
            }
            return http.Response(
              jsonEncode([
                {
                  'invoice_number': 'INV-0001',
                  'customer_id': 'acme',
                  'invoice_date': '2026-09-01',
                  'due_date': '2026-09-30',
                  'total_amount': '250.000000',
                  'paid_amount': '100.000000',
                  'status': 'issued',
                  'items': const [],
                },
              ]),
              200,
            );
          }),
        );

      await tester.pumpWidget(
        MaterialApp(
          home: CustomerStatementPage(
            customerId: 'acme',
            customerName: 'Acme Ltd',
            service: service,
            today: today,
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('150.00 owed as at 2026-11-18'), findsOneWidget);
      expect(
        find.textContaining('250.00 billed, 100.00 received'),
        findsOneWidget,
      );
      expect(find.text('INV-0001 raised, due 2026-09-30'), findsOneWidget);
      expect(find.text('Payment against INV-0001'), findsOneWidget);
      expect(find.text('−100.00'), findsOneWidget);
      expect(
        find.textContaining(
          '150.00 of that is past its date, the oldest 49 days',
        ),
        findsOneWidget,
      );
    },
  );
}
