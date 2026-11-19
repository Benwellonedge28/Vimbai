// Taking a sale back.
//
// The return is the engine's to post: it knows which account a return goes to, what each tax was charged to and where it
// is owed. What is pinned here is that the phone sends the document exactly as it stands - the full figure, not the net of
// it, and the taxes by name - and that a refusal is repeated in the engine's own words rather than as a status code.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/sales_return.dart';

void main() {
  test(
    'a return sends the whole figure and the taxes the sale carried',
    () async {
      String? sent;
      final service = SalesReturnService.fresh()
        ..useClient(
          MockClient((request) async {
            sent = request.body;
            return http.Response(
              '{"return_number":"CN-CAR-1","total_amount":"100000.00",'
              '"tax_amount":"13419.91","journal_entry_id":"je-9"}',
              201,
            );
          }),
        );

      final posted = await service.record(
        entryDate: '2026-11-20',
        returnNumber: 'CN-CAR-1',
        originalInvoiceNumber: 'CAR-1',
        customerId: 'c-1',
        total: ExactAmount.parse('100000.00'),
        taxes: const ['VAT'],
        reason: 'came back',
      );

      final body = jsonDecode(sent!) as Map<String, dynamic>;
      // the customer paid 100,000, so 100,000 is what is credited back: the tax is taken out of it, never added to it
      expect(body['total_amount'], '100000.00');
      expect(body['taxes'], ['VAT']);
      expect(body['original_invoice_number'], 'CAR-1');
      expect(body['customer_id'], 'c-1');
      expect(body['reason'], 'came back');
      expect(posted.taxAmount, '13419.91');
      expect(posted.journalEntryId, 'je-9');
    },
  );

  test('a refusal is repeated in the engine\'s words', () async {
    final service = SalesReturnService.fresh()
      ..useClient(
        MockClient(
          (_) async => http.Response(
            '{"detail":"VAT is not a tax this Book has defined. Add it first."}',
            422,
          ),
        ),
      );

    await expectLater(
      service.record(
        entryDate: '2026-11-20',
        returnNumber: 'CN-1',
        originalInvoiceNumber: 'CAR-1',
        customerId: 'c-1',
        total: ExactAmount.parse('10.00'),
        taxes: const ['VAT'],
      ),
      throwsA(
        predicate(
          (failure) => failure.toString().contains(
            'VAT is not a tax this Book has defined',
          ),
        ),
      ),
    );
  });
}
