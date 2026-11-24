// Goods back to a supplier.
//
// A return is the purchase turned round: what the supplier owes falls, the cost comes back out, and no money moves. What
// this pins is that the gross credit reaches the engine as the supplier showed it, that tax comes out of that gross rather
// than being added to it, and that a foreign credit carries its money and rate with nothing converted on the device.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/banking/data/currencies.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';
import 'package:vimbai_mobile_client/features/payables/data/purchase_return_posting.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/return_to_supplier_page.dart';

/// The suppliers the Book already owes.
class _FakePayables extends PayablesService {
  _FakePayables() : super.fresh();

  @override
  Future<PayablesPosition> position() async => PayablesPosition(
    suppliers: const [
      OwedToSupplier(
        supplierId: 'timberco',
        name: 'Timber Co',
        balance: ExactAmount.zero,
        movements: 2,
        lastActivity: '2026-11-01',
      ),
    ],
    total: ExactAmount.zero,
  );
}

/// A Book that keeps dollars beside its own money.
class _FakeCurrencies extends CurrencyService {
  _FakeCurrencies() : super.fresh();

  @override
  Future<CurrencyBook> read() async => const CurrencyBook([
    IsoCurrency(code: 'USD', name: 'US Dollar', decimalPlaces: 2),
  ]);
}

/// Stands in for the engine's purchases returns journal: it remembers what it was asked to post.
class _FakeReturns extends PurchaseReturnPostingService {
  _FakeReturns() : super.fresh();

  final List<Map<String, Object?>> posted = [];

  @override
  Future<PurchaseReturnPosted> post({
    required String entryDate,
    required String returnNumber,
    required String vendorId,
    required ExactAmount total,
    ExactAmount? tax,
    String originalOrder = '',
    String reason = '',
    String currency = '',
    String rate = '',
    String notes = '',
  }) async {
    posted.add({
      'entry_date': entryDate,
      'reference': returnNumber,
      'vendor': vendorId,
      'total': total.toDecimalString(),
      'tax': tax?.toDecimalString(),
      'original_order': originalOrder,
      'reason': reason,
      'currency': currency,
      'rate': rate,
    });
    return PurchaseReturnPosted(
      returnNumber: returnNumber,
      totalAmount: '12750.00',
      journalEntryId: 'entry-1',
    );
  }
}

Future<void> _fill(WidgetTester tester, String label, String text) async {
  await tester.enterText(find.widgetWithText(TextField, label), text);
  await tester.pumpAndSettle();
}

Future<void> _record(WidgetTester tester) async {
  await tester.tap(find.text('Record the return'));
  await tester.pumpAndSettle();
}

Future<void> _open(
  WidgetTester tester, {
  required _FakeReturns returns,
  String vendorId = '',
}) async {
  // the form is taller than a phone, so it is given a tall surface: every field is on screen and nothing is hidden
  tester.view.physicalSize = const Size(1000, 2600);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      home: ReturnToSupplierPage(
        posting: returns,
        payables: _FakePayables(),
        currencies: _FakeCurrencies(),
        vendorId: vendorId,
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'a return in the Book\'s own money sends no currency and no rate',
    (tester) async {
      final returns = _FakeReturns();
      await _open(tester, returns: returns, vendorId: 'timberco');

      await _fill(tester, 'Your reference for this return', 'RT-1');
      await _fill(tester, 'What they are crediting back', '4000');
      await _record(tester);

      final posted = returns.posted.single;
      expect(posted['reference'], 'RT-1');
      expect(posted['vendor'], 'timberco');
      expect(posted['total'], '4000.00');
      expect(
        posted['currency'],
        '',
        reason: 'the Book\'s own money names nothing',
      );
      expect(posted['rate'], '');
    },
  );

  testWidgets('the tax is carried inside the credit, not added to it', (
    tester,
  ) async {
    final returns = _FakeReturns();
    await _open(tester, returns: returns, vendorId: 'timberco');

    await _fill(tester, 'Your reference for this return', 'RT-2');
    await _fill(tester, 'What they are crediting back', '1150');
    await _fill(tester, 'Tax inside that figure (if any)', '150');
    await _record(tester);

    final posted = returns.posted.single;
    expect(posted['total'], '1150.00', reason: 'the gross credit as it stands');
    expect(posted['tax'], '150.00');
  });

  testWidgets(
    'a credit in dollars carries the money, the rate and the supplier\'s figure',
    (tester) async {
      final returns = _FakeReturns();
      await _open(tester, returns: returns, vendorId: 'timberco');

      await _fill(tester, 'Your reference for this return', 'RT-3');
      await _fill(tester, 'What they are crediting back', '500');
      await _fill(tester, 'The purchase it reverses (if known)', 'PO-13');
      await tester.tap(find.text('This Book\'s own money'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('USD · US Dollar').last);
      await tester.pumpAndSettle();
      await _fill(tester, 'Rate on the day of the return', '25.5');
      await _record(tester);

      final posted = returns.posted.single;
      expect(
        posted['total'],
        '500.00',
        reason: 'what the supplier showed, unconverted',
      );
      expect(posted['currency'], 'USD');
      expect(posted['rate'], '25.5');
      expect(posted['original_order'], 'PO-13');
    },
  );

  testWidgets(
    'a return in another money without a rate is refused before it is sent',
    (tester) async {
      final returns = _FakeReturns();
      await _open(tester, returns: returns, vendorId: 'timberco');

      await _fill(tester, 'Your reference for this return', 'RT-4');
      await _fill(tester, 'What they are crediting back', '500');
      await tester.tap(find.text('This Book\'s own money'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('USD · US Dollar').last);
      await tester.pumpAndSettle();
      await _record(tester);

      expect(find.textContaining('Say what one USD is worth'), findsOneWidget);
      expect(returns.posted, isEmpty);
    },
  );

  testWidgets(
    'a return with no supplier or no reference is refused, naming what is missing',
    (tester) async {
      final returns = _FakeReturns();
      await _open(tester, returns: returns);

      await _fill(tester, 'What they are crediting back', '500');
      await _record(tester);
      expect(find.textContaining('which supplier'), findsOneWidget);

      await _fill(tester, 'Supplier', 'timberco');
      await _record(tester);
      expect(find.textContaining('reference of your own'), findsOneWidget);
      expect(returns.posted, isEmpty);
    },
  );

  testWidgets(
    'tax bigger than the credit is refused rather than posting a negative cost',
    (tester) async {
      final returns = _FakeReturns();
      await _open(tester, returns: returns, vendorId: 'timberco');

      await _fill(tester, 'Your reference for this return', 'RT-5');
      await _fill(tester, 'What they are crediting back', '100');
      await _fill(tester, 'Tax inside that figure (if any)', '150');
      await _record(tester);

      expect(
        find.textContaining('cannot be bigger than the credit'),
        findsOneWidget,
      );
      expect(returns.posted, isEmpty);
    },
  );
}
