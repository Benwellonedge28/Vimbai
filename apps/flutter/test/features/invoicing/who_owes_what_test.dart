// Who owes what: the grouping, and the screen that shows it.
//
// The grouping is the whole point of the view, so it is tested directly: settled invoices must not appear at all, two
// invoices from the same customer must add up rather than list twice, and the biggest debt comes first because that is the
// one worth ringing about. A screen that grouped wrongly would send somebody chasing the wrong person.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/who_owes_what_page.dart';

Invoice _invoice({
  required String number,
  required String customer,
  required String total,
  String paid = '0',
  String due = '2026-12-18',
}) => Invoice(
  invoiceNumber: number,
  customerId: customer,
  invoiceDate: '2026-11-01',
  dueDate: due,
  total: ExactAmount.parse(total),
  paid: ExactAmount.parse(paid),
  status: 'outstanding',
  lines: const [],
);

void main() {
  test(
    'one customer owing on two invoices is one row, added up, biggest first',
    () {
      final rows = owingByCustomer(
        [
          _invoice(
            number: 'INV-1',
            customer: 'acme',
            total: '100',
            due: '2026-12-01',
          ),
          _invoice(number: 'INV-2', customer: 'acme', total: '50', paid: '50'),
          _invoice(number: 'INV-3', customer: 'beta', total: '400'),
        ],
        names: const {'acme': 'Acme Ltd'},
      );

      expect(rows.length, 2);
      expect(rows.first.customerId, 'beta');
      expect(rows.first.owed, ExactAmount.parse('400'));
      expect(rows.last.name, 'Acme Ltd');
      expect(
        rows.last.owed,
        ExactAmount.parse('100'),
        reason: 'the settled invoice must not be added in',
      );
      expect(rows.last.invoices, 1);
      expect(rows.last.earliestDue, '2026-12-01');
    },
  );

  test(
    'a customer whose invoices are all settled is not on the list at all',
    () {
      final rows = owingByCustomer([
        _invoice(number: 'INV-1', customer: 'acme', total: '100', paid: '100'),
      ]);
      expect(rows, isEmpty);
    },
  );

  testWidgets(
    'the screen names who owes, and does not show an unreadable register as nobody owing',
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
        MaterialApp(home: WhoOwesWhatPage(service: service)),
      );
      await tester.pumpAndSettle();

      expect(find.byType(CouldNotRead), findsOneWidget);
      expect(find.textContaining('Nobody owes anything'), findsNothing);
    },
  );

  testWidgets('a customer with money owing is named with the figure', (
    tester,
  ) async {
    final service = InvoicingService.fresh()
      ..useClient(
        MockClient((request) async {
          if (request.url.path.endsWith('/customers')) {
            return http.Response(
              '[{"id":"c1","customer_id":"acme","name":"Acme Ltd"}]',
              200,
            );
          }
          return http.Response(
            '[{"invoice_number":"INV-0001","customer_id":"acme","invoice_date":"2026-11-01",'
            '"due_date":"2026-12-01","total_amount":"779.985","paid_amount":"100",'
            '"status":"outstanding","items":[]}]',
            200,
          );
        }),
      );

    await tester.pumpWidget(
      MaterialApp(home: WhoOwesWhatPage(service: service)),
    );
    await tester.pumpAndSettle();

    expect(find.text('Acme Ltd'), findsOneWidget);
    expect(find.textContaining('679.985'), findsOneWidget);
    expect(find.textContaining('due 2026-12-01'), findsOneWidget);
  });
}
