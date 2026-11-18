// Invoices: the figure that is sent, and what a screen says when the register cannot be read.
//
// The engine refuses a total that disagrees with its lines to the cent, so the total has to be the sum of what was typed,
// in exact text. And a register that could not be read must never look like a register with nothing in it: those two
// states mean opposite things to somebody deciding whether they have billed anyone.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/invoices_page.dart';

const _customer = InvoiceCustomer(
  id: 'row-1',
  customerId: 'acme',
  name: 'Acme Ltd',
);

Invoice _held(String number) => Invoice(
  invoiceNumber: number,
  customerId: 'acme',
  invoiceDate: '2026-11-01',
  dueDate: '2026-12-01',
  total: ExactAmount.zero,
  paid: ExactAmount.zero,
  status: 'outstanding',
  lines: const [],
);

void main() {
  setUp(() => SessionToken.set('a-session-token'));
  tearDown(() => SessionToken.clear());

  test(
    'the total sent is the sum of the lines, in exact six-place text',
    () async {
      late Map<String, dynamic> sent;
      final service = InvoicingService.fresh()
        ..useClient(
          MockClient((request) async {
            sent = jsonDecode(request.body) as Map<String, dynamic>;
            return http.Response(jsonEncode(sent), 200);
          }),
        );

      await service.raiseInvoice(
        invoiceNumber: 'INV-0001',
        customer: _customer,
        invoiceDate: DateTime(2026, 11, 18),
        dueDate: DateTime(2026, 12, 18),
        lines: [
          InvoiceLine(
            description: 'Consulting',
            quantity: ExactAmount.parse('3'),
            unitPrice: ExactAmount.parse('250.00'),
          ),
          InvoiceLine(
            description: 'Travel',
            quantity: ExactAmount.parse('1.5'),
            unitPrice: ExactAmount.parse('19.99'),
          ),
        ],
      );

      expect(sent['total_amount'], '779.985000', reason: '$sent');
      expect(sent['customer_id'], 'acme');
      expect(sent['status'], 'outstanding');
      expect(sent['invoice_date'], '2026-11-18');
      expect(sent['due_date'], '2026-12-18');
      final items = sent['items'] as List;
      expect(items.first['amount'], '750.000000');
      expect(items.first['quantity'], '3.000000');
      expect(items.first['unit_price'], '250.000000');
      expect(items.last['amount'], '29.985000');
    },
  );

  test(
    'an invoice whose lines come to nothing is refused before it is sent',
    () async {
      var sent = false;
      final service = InvoicingService.fresh()
        ..useClient(
          MockClient((request) async {
            sent = true;
            return http.Response('{}', 200);
          }),
        );

      await expectLater(
        service.raiseInvoice(
          invoiceNumber: 'INV-0002',
          customer: _customer,
          invoiceDate: DateTime(2026, 11, 18),
          dueDate: DateTime(2026, 12, 18),
          lines: [
            InvoiceLine(
              description: 'Consulting',
              quantity: ExactAmount.parse('2'),
              unitPrice: ExactAmount.zero,
            ),
          ],
        ),
        throwsA(
          predicate(
            (error) => error.toString().contains('would bill 0.00'),
            'names the zero bill',
          ),
        ),
      );
      expect(sent, isFalse, reason: 'nothing reached the engine');
    },
  );

  test('the number offered is one past the highest already taken', () {
    final service = InvoicingService.fresh();
    expect(service.nextNumberFor([]), 'INV-0001');
    expect(service.nextNumberFor([_held('INV-0007')]), 'INV-0008');
    expect(
      service.nextNumberFor([_held('INV-0002'), _held('A-0009')]),
      'INV-0010',
    );
  });

  test(
    'a customer is looked up by the identifier the invoice points at',
    () async {
      final asked = <String>[];
      final service = InvoicingService.fresh()
        ..useClient(
          MockClient((request) async {
            asked.add(request.body);
            return http.Response(
              jsonEncode({
                'id': 'row-2',
                'customer_id': 'acme-ltd',
                'name': 'Acme Ltd',
              }),
              200,
            );
          }),
        );
      final customer = await service.createCustomer('  Acme Ltd  ');
      expect(customer.customerId, 'acme-ltd');
      expect(jsonDecode(asked.single), {
        'customer_id': 'acme-ltd',
        'name': 'Acme Ltd',
      });
    },
  );

  testWidgets(
    'a register that could not be read says so, and does not pretend to be empty',
    (tester) async {
      final service = InvoicingService.fresh()
        ..useClient(
          MockClient(
            (_) async => http.Response(
              '{"error":"No active membership in book x"}',
              403,
            ),
          ),
        );

      await tester.pumpWidget(
        MaterialApp(home: InvoicesPage(service: service)),
      );
      await tester.pumpAndSettle();

      expect(find.byType(CouldNotRead), findsOneWidget);
      expect(find.textContaining('No invoices raised yet'), findsNothing);
      expect(find.text('Try again'), findsOneWidget);
    },
  );

  testWidgets(
    'an empty register explains itself and offers the first invoice',
    (tester) async {
      final service = InvoicingService.fresh()
        ..useClient(MockClient((_) async => http.Response('[]', 200)));

      await tester.pumpWidget(
        MaterialApp(home: InvoicesPage(service: service)),
      );
      await tester.pumpAndSettle();

      expect(find.byType(CouldNotRead), findsNothing);
      expect(find.textContaining('No invoices raised yet'), findsOneWidget);
      expect(
        find.textContaining('does not post to the ledger'),
        findsOneWidget,
      );
    },
  );
}
