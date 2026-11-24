// Stock bought on account.
//
// A credit purchase is not a payment and not a bill received: the stock arrives, the supplier is owed, and no money moves.
// What this pins is that the purchase reaches the engine with the supplier's own figure and, when the supplier trades in
// another money, the money and the rate, with nothing converted on the device.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/banking/data/currencies.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';
import 'package:vimbai_mobile_client/features/payables/data/purchase_posting.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/buy_stock_page.dart';

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

/// Stands in for the engine's purchases journal: it remembers what it was asked to post.
class _FakePurchases extends PurchasePostingService {
  _FakePurchases() : super.fresh();

  final List<Map<String, Object?>> posted = [];

  @override
  Future<PurchasePosted> post({
    required String entryDate,
    required String orderNumber,
    required String vendorId,
    required ExactAmount total,
    ExactAmount? discount,
    ExactAmount? tax,
    String dueDate = '',
    String currency = '',
    String rate = '',
    String notes = '',
  }) async {
    posted.add({
      'entry_date': entryDate,
      'order': orderNumber,
      'vendor': vendorId,
      'total': total.toDecimalString(),
      'discount': discount?.toDecimalString(),
      'tax': tax?.toDecimalString(),
      'due_date': dueDate,
      'currency': currency,
      'rate': rate,
    });
    return PurchasePosted(
      orderNumber: orderNumber,
      totalAmount: '25500.00',
      journalEntryId: 'entry-1',
    );
  }
}

/// Types into the field labelled [label].
Future<void> _fill(WidgetTester tester, String label, String text) async {
  await tester.enterText(find.widgetWithText(TextField, label), text);
  await tester.pumpAndSettle();
}

/// Records the purchase, which is the last thing the form offers.
Future<void> _record(WidgetTester tester) async {
  await tester.tap(find.text('Record the purchase'));
  await tester.pumpAndSettle();
}

Future<void> _open(
  WidgetTester tester, {
  required _FakePurchases purchases,
  String vendorId = '',
}) async {
  // the form is taller than a phone, so it is given a tall surface: every field is on screen and nothing is hidden
  tester.view.physicalSize = const Size(1000, 2600);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      home: BuyStockPage(
        posting: purchases,
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
    'a purchase in the Book\'s own money sends no currency and no rate',
    (tester) async {
      final purchases = _FakePurchases();
      await _open(tester, purchases: purchases, vendorId: 'timberco');

      await _fill(tester, 'Their reference', 'PO-12');
      await _fill(tester, 'What it came to', '4000');
      await tester.pumpAndSettle();
      await _record(tester);

      final posted = purchases.posted.single;
      expect(posted['order'], 'PO-12');
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

  testWidgets(
    'a purchase in dollars carries the money, the rate and the supplier\'s figure',
    (tester) async {
      final purchases = _FakePurchases();
      await _open(tester, purchases: purchases, vendorId: 'timberco');

      await _fill(tester, 'Their reference', 'PO-13');
      await _fill(tester, 'What it came to', '1000');
      await _fill(tester, 'Discount (if any)', '100');
      await tester.tap(find.text('This Book\'s own money'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('USD · US Dollar').last);
      await tester.pumpAndSettle();
      await _fill(tester, 'Rate on the day of the purchase', '25.5');
      await tester.pumpAndSettle();
      await _record(tester);

      final posted = purchases.posted.single;
      expect(
        posted['total'],
        '1000.00',
        reason: 'what the supplier showed, unconverted',
      );
      expect(posted['discount'], '100.00');
      expect(posted['currency'], 'USD');
      expect(posted['rate'], '25.5');
    },
  );

  testWidgets(
    'a purchase in another money without a rate is refused before it is sent',
    (tester) async {
      final purchases = _FakePurchases();
      await _open(tester, purchases: purchases, vendorId: 'timberco');

      await _fill(tester, 'Their reference', 'PO-14');
      await _fill(tester, 'What it came to', '1000');
      await tester.tap(find.text('This Book\'s own money'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('USD · US Dollar').last);
      await tester.pumpAndSettle();
      await _record(tester);

      expect(find.textContaining('Say what one USD is worth'), findsOneWidget);
      expect(purchases.posted, isEmpty);
    },
  );

  testWidgets(
    'a purchase with no supplier or no reference is refused, naming what is missing',
    (tester) async {
      final purchases = _FakePurchases();
      await _open(tester, purchases: purchases);

      await _fill(tester, 'What it came to', '500');
      await _record(tester);
      expect(find.textContaining('which supplier'), findsOneWidget);

      await _fill(tester, 'Supplier', 'timberco');
      await tester.pumpAndSettle();
      await _record(tester);
      expect(find.textContaining('own reference'), findsOneWidget);
      expect(purchases.posted, isEmpty);
    },
  );

  testWidgets(
    'a supplier already on the books can be picked rather than typed',
    (tester) async {
      final purchases = _FakePurchases();
      await _open(tester, purchases: purchases);

      await tester.tap(find.text('Supplier already on the books'));
      await tester.pumpAndSettle();
      await tester.tap(find.textContaining('Timber Co').last);
      await tester.pumpAndSettle();

      await _fill(tester, 'Their reference', 'PO-15');
      await _fill(tester, 'What it came to', '250');
      await tester.pumpAndSettle();
      await _record(tester);

      expect(purchases.posted.single['vendor'], 'timberco');
    },
  );
}
