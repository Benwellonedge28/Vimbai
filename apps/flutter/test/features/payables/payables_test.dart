// What you owe, held to the ledger it came from.
//
// Two things matter and both are about not lying to the person reading the page: the total is the ledger's own figure rather
// than a sum done on the phone, and the refusals happen before anything is sent, because the engine would faithfully accept a
// payment larger than the balance and leave that supplier owed a negative amount.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/payables_page.dart';

const String _position = '{"creditors":['
    '{"creditor_id":"paperco","creditor_name":"Paper Co","balance":"300.000000","transactions":2,"last_activity":"2026-11-05"},'
    '{"creditor_id":"inkco","creditor_name":"Ink Supplies","balance":"95.500000","transactions":1,"last_activity":"2026-11-12"}'
    '],"total_balance":"395.500000"}';

const String _agreed =
    '{"control_account_balance":"395.500000","purchases_ledger_total":"395.500000","difference":"0.000000","reconciled":true}';

PayablesService _service({String? position, String? check, String? record}) {
  final requests = <http.Request>[];
  final service = PayablesService.fresh()
    ..useClient(
      MockClient((request) async {
        requests.add(request as http.Request);
        final path = request.url.path;
        if (path.endsWith('/creditors')) {
          return http.Response(position ?? _position, 200);
        }
        if (path.endsWith('/reconcile')) {
          return http.Response(check ?? _agreed, 200);
        }
        return http.Response(
          record ??
              '{"creditor_id":"paperco","creditor_name":"Paper Co","transaction_type":"payment",'
                  '"amount":"180.000000","date":"2026-11-18","balance":"120.000000"}',
          200,
        );
      }),
    );
  service.__requests = requests;
  return service;
}

extension on PayablesService {
  static final Expando<List<http.Request>> _sent = Expando();

  set __requests(List<http.Request> value) => _sent[this] = value;

  List<http.Request> get sent => _sent[this] ?? const [];
}

void main() {
  test('the suppliers come largest first, and the total is the ledger\'s own', () async {
    final service = _service();
    final position = await service.position();

    expect(position.suppliers.first.name, 'Paper Co');
    expect(position.suppliers.first.balance.toDecimalString(), '300.00');
    expect(position.suppliers.last.name, 'Ink Supplies');
    expect(position.total.toDecimalString(), '395.50');
  });

  test('a supplier with no name still reads as somebody', () async {
    final position = PayablesPosition.fromJson({
      'creditors': [
        {
          'creditor_id': 'unnamed-co',
          'creditor_name': 'unnamed-co',
          'balance': '20.000000',
          'transactions': 1,
          'last_activity': '2026-11-12',
        },
      ],
      'total_balance': '20.000000',
    });
    expect(position.suppliers.single.name, 'unnamed-co');
  });

  test('a movement of nothing never reaches the engine', () async {
    final service = _service();
    await expectLater(
      service.record(
        supplierId: 'paperco',
        supplierName: 'Paper Co',
        movement: SupplierMovement.payment,
        amount: ExactAmount.zero,
        outstandingBefore: ExactAmount.parse('300'),
      ),
      throwsA(
        predicate((failure) => '$failure'.contains('changes nothing')),
      ),
    );
    expect(service.sent, isEmpty);
  });

  test('a payment bigger than what is owed is refused before it is sent', () async {
    final service = _service();
    await expectLater(
      service.record(
        supplierId: 'paperco',
        supplierName: 'Paper Co',
        movement: SupplierMovement.payment,
        amount: ExactAmount.parse('400'),
        outstandingBefore: ExactAmount.parse('300'),
      ),
      throwsA(predicate((failure) => '$failure'.contains('only owe 300.00'))),
    );
    expect(service.sent, isEmpty);
  });

  test('a bill received may be any amount, because nothing is owed yet', () async {
    final service = _service();
    final saved = await service.record(
      supplierId: 'newco',
      supplierName: 'New Co',
      movement: SupplierMovement.bill,
      amount: ExactAmount.parse('1250'),
      outstandingBefore: null,
      reference: 'BILL-1',
    );
    expect(saved.balance.toDecimalString(), '120.00');
    final sent = jsonDecode(service.sent.single.body) as Map<String, dynamic>;
    expect(sent['transaction_type'], 'purchase_invoice');
    expect(sent['amount'], '1250.000000');
    expect(sent['creditor_name'], 'New Co');
    expect(sent['reference'], 'BILL-1');
  });

  testWidgets('the page leads with the total owed and says the books agree', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: PayablesPage(service: _service(), today: '2026-11-18'),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('395.50 owed to 2 suppliers'), findsOneWidget);
    expect(find.text('Paper Co'), findsOneWidget);
    expect(find.text('2 movements, last on 2026-11-05'), findsOneWidget);
    expect(find.textContaining('purchases control account agrees'), findsOneWidget);
  });

  testWidgets('a control account that disagrees is said plainly, not hidden', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: PayablesPage(
          service: _service(
            check:
                '{"control_account_balance":"500.000000","purchases_ledger_total":"395.500000",'
                '"difference":"104.500000","reconciled":false}',
          ),
          today: '2026-11-18',
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('a difference of 104.50'), findsOneWidget);
    expect(find.textContaining('agrees'), findsNothing);
  });

  testWidgets('an empty Book explains what the page is for', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: PayablesPage(
          service: _service(
            position: '{"creditors":[],"total_balance":"0.000000"}',
          ),
          today: '2026-11-18',
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('You owe no supplier on this Book.'), findsOneWidget);
  });
}
