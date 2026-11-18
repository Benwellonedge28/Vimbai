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
import 'package:vimbai_mobile_client/features/payables/presentation/supplier_account_page.dart';

const String _listed =
    '{"creditors":['
    '{"creditor_id":"paperco","creditor_name":"Paper Co","balance":"300.000000","transactions":2,"last_activity":"2026-11-05"},'
    '{"creditor_id":"inkco","creditor_name":"Ink Supplies","balance":"95.500000","transactions":1,"last_activity":"2026-11-12"}'
    '],"total_balance":"395.500000"}';

const String _agreed =
    '{"control_account_balance":"395.500000","purchases_ledger_total":"395.500000","difference":"0.000000","reconciled":true}';

/// A service answering from canned engine answers, and everything it was asked.
class _Fake {
  _Fake({String? position, String? check, String? record})
    : _position = position,
      _check = check,
      _record = record;

  final String? _position;
  final String? _check;
  final String? _record;

  /// Every request the page and the service actually sent.
  final List<http.Request> sent = [];

  late final PayablesService service = PayablesService.fresh()
    ..useClient(
      MockClient((request) async {
        sent.add(request);
        final path = request.url.path;
        if (path.endsWith('/creditors')) {
          return http.Response(_position ?? _listed, 200);
        }
        if (path.endsWith('/reconcile')) {
          return http.Response(_check ?? _agreed, 200);
        }
        return http.Response(
          _record ??
              '{"creditor_id":"paperco","creditor_name":"Paper Co","transaction_type":"payment",'
                  '"amount":"180.000000","date":"2026-11-18","balance":"120.000000"}',
          200,
        );
      }),
    );
}

void main() {
  test(
    'the suppliers come largest first, and the total is the ledger\'s own',
    () async {
      final fake = _Fake();
      final service = fake.service;
      final position = await service.position();

      expect(position.suppliers.first.name, 'Paper Co');
      expect(position.suppliers.first.balance.toDecimalString(), '300.00');
      expect(position.suppliers.last.name, 'Ink Supplies');
      expect(position.total.toDecimalString(), '395.50');
    },
  );

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
    final fake = _Fake();
    final service = fake.service;
    await expectLater(
      service.record(
        supplierId: 'paperco',
        supplierName: 'Paper Co',
        movement: SupplierMovement.payment,
        amount: ExactAmount.zero,
        outstandingBefore: ExactAmount.parse('300'),
      ),
      throwsA(predicate((failure) => '$failure'.contains('changes nothing'))),
    );
    expect(fake.sent, isEmpty);
  });

  test(
    'a payment bigger than what is owed is refused before it is sent',
    () async {
      final fake = _Fake();
      final service = fake.service;
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
      expect(fake.sent, isEmpty);
    },
  );

  test(
    'a bill received may be any amount, because nothing is owed yet',
    () async {
      final fake = _Fake();
      final service = fake.service;
      final saved = await service.record(
        supplierId: 'newco',
        supplierName: 'New Co',
        movement: SupplierMovement.bill,
        amount: ExactAmount.parse('1250'),
        outstandingBefore: null,
        reference: 'BILL-1',
      );
      expect(saved.balance.toDecimalString(), '120.00');
      final sent = jsonDecode(fake.sent.single.body) as Map<String, dynamic>;
      expect(sent['transaction_type'], 'purchase_invoice');
      expect(sent['amount'], '1250.000000');
      expect(sent['creditor_name'], 'New Co');
      expect(sent['reference'], 'BILL-1');
    },
  );

  testWidgets('the page leads with the total owed and says the books agree', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: PayablesPage(service: _Fake().service, today: '2026-11-18'),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('395.50 owed to 2 suppliers'), findsOneWidget);
    expect(find.text('Paper Co'), findsOneWidget);
    expect(find.text('2 movements, last on 2026-11-05'), findsOneWidget);
    expect(
      find.textContaining('purchases control account agrees'),
      findsOneWidget,
    );
  });

  testWidgets(
    'tapping a supplier opens their account, and the button beside it records a movement',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          home: PayablesPage(service: _Fake().service, today: '2026-11-18'),
        ),
      );
      await tester.pumpAndSettle();

      // the name is what a person taps to ask "how did we get to 300.00?"
      await tester.tap(find.text('Paper Co'));
      await tester.pumpAndSettle();
      expect(find.byType(SupplierAccountPage), findsOneWidget);

      // and recording is still reachable, but deliberately a separate act
      await tester.pageBack();
      await tester.pumpAndSettle();
      expect(find.byIcon(Icons.add_circle_outline), findsWidgets);
    },
  );

  testWidgets('a control account that disagrees is said plainly, not hidden', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: PayablesPage(
          service: _Fake(
            check:
                '{"control_account_balance":"500.000000","purchases_ledger_total":"395.500000",'
                '"difference":"104.500000","reconciled":false}',
          ).service,
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
          service: _Fake(
            position: '{"creditors":[],"total_balance":"0.000000"}',
          ).service,
          today: '2026-11-18',
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('You owe no supplier on this Book.'), findsOneWidget);
  });
}
