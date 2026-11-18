// Everyone you bill: the register's arithmetic and the page that keeps it clean.
//
// The cases that matter are the ones a demo would skip: a customer who has never been billed (removable) against one who has
// paid in full (not removable, because their invoice still points at them), two spellings of one name, and a failed read.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/customer_register.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/customer_register_page.dart';

InvoiceCustomer _customer(String id, String name, {String email = ''}) =>
    InvoiceCustomer(id: 'row-$id', customerId: id, name: name, email: email);

Invoice _invoice(
  String number,
  String customerId,
  String total, {
  String paid = '0',
  String date = '2026-11-01',
}) => Invoice(
  invoiceNumber: number,
  customerId: customerId,
  invoiceDate: date,
  dueDate: '2026-11-30',
  total: ExactAmount.parse(total),
  paid: ExactAmount.parse(paid),
  status: 'issued',
  lines: const [],
);

class _Served implements InvoicingService {
  _Served({required this.customers, required this.invoices});

  List<InvoiceCustomer> customers;
  List<Invoice> invoices;
  final List<String> created = [];
  final List<String> updated = [];
  final List<String> removed = [];

  @override
  Future<List<InvoiceCustomer>> listCustomers() async => customers;

  @override
  Future<List<Invoice>> listInvoices() async => invoices;

  @override
  Future<InvoiceCustomer> createCustomer(String name) async {
    created.add(name);
    final made = _customer('c-${customers.length + 1}', name);
    customers = [...customers, made];
    return made;
  }

  @override
  Future<InvoiceCustomer> updateCustomer(
    String customerId, {
    required String name,
    String? email,
    String? phone,
    String? address,
  }) async {
    updated.add('$customerId:$name');
    return _customer(customerId, name);
  }

  @override
  Future<void> removeCustomer(String customerId) async {
    removed.add(customerId);
    customers = [
      for (final customer in customers)
        if (customer.customerId != customerId) customer,
    ];
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _Fails implements InvoicingService {
  @override
  Future<List<InvoiceCustomer>> listCustomers() async =>
      throw Exception('the engine answered 503 while it restarts');

  @override
  Future<List<Invoice>> listInvoices() async => const [];

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  group('the register', () {
    test('every customer is on it, owing or not', () {
      final rows = registerOf(
        [
          _customer('a', 'Acme'),
          _customer('b', 'Bright'),
          _customer('c', 'Cove'),
        ],
        [_invoice('INV-1', 'a', '300.00')],
      );
      expect(rows.map((row) => row.name), ['Acme', 'Bright', 'Cove']);
      expect(rows.first.owed, ExactAmount.parse('300.00'));
      expect(rows[1].owed.isZero, isTrue);
      expect(rows[1].neverBilled, isTrue);
    });

    test(
      'what a customer owes is summed across their invoices, not stored',
      () {
        final rows = registerOf(
          [_customer('a', 'Acme')],
          [
            _invoice('INV-1', 'a', '300.00', paid: '120.00'),
            _invoice('INV-2', 'a', '455.00'),
          ],
        );
        expect(rows.single.owed, ExactAmount.parse('635.00'));
        expect(rows.single.invoices, 2);
        expect(rows.single.lastBilledOn, '2026-11-01');
      },
    );

    test(
      'a customer who paid in full owes nothing but is not never-billed',
      () {
        final rows = registerOf(
          [_customer('a', 'Acme')],
          [_invoice('INV-1', 'a', '300.00', paid: '300.00')],
        );
        expect(rows.single.owed.isZero, isTrue);
        expect(rows.single.neverBilled, isFalse);
        expect(rows.single.removalRefusal, isNotNull);
      },
    );

    test('the largest debt leads, then names alphabetically', () {
      final rows = registerOf(
        [
          _customer('a', 'Zeta'),
          _customer('b', 'Acme'),
          _customer('c', 'Mine'),
        ],
        [_invoice('INV-1', 'a', '50.00'), _invoice('INV-2', 'b', '900.00')],
      );
      expect(rows.map((row) => row.name), ['Acme', 'Zeta', 'Mine']);
    });

    test('a customer who has never been billed may be removed', () {
      final rows = registerOf([_customer('a', 'Acme')], const []);
      expect(rows.single.removalRefusal, isNull);
    });

    test('a customer with invoices may not, and the reason says how many', () {
      final rows = registerOf(
        [_customer('a', 'Acme')],
        [_invoice('INV-1', 'a', '10.00'), _invoice('INV-2', 'a', '10.00')],
      );
      expect(rows.single.removalRefusal, contains('2 invoices'));
      expect(rows.single.removalRefusal, contains('Acme'));
    });

    test('one invoice is said in the singular', () {
      final rows = registerOf(
        [_customer('a', 'Acme')],
        [_invoice('INV-1', 'a', '10.00')],
      );
      expect(rows.single.removalRefusal, contains('1 invoice on file'));
      expect(rows.single.removalRefusal, contains('that invoice'));
    });

    test('a draft invoice is not a debt: nobody has been asked for it', () {
      final draft = Invoice(
        invoiceNumber: 'INV-9',
        customerId: 'a',
        invoiceDate: '2026-11-01',
        dueDate: '2026-11-30',
        total: ExactAmount.parse('500.00'),
        paid: ExactAmount.zero,
        status: 'draft',
        lines: const [],
      );
      final rows = registerOf([_customer('a', 'Acme')], [draft]);
      expect(rows.single.owed.isZero, isTrue);
      expect(rows.single.invoices, 1, reason: 'it is still on their record');
      expect(owedAcross([draft]).isZero, isTrue);
    });

    test('a voided invoice is not a debt either', () {
      final voided = Invoice(
        invoiceNumber: 'INV-8',
        customerId: 'a',
        invoiceDate: '2026-11-01',
        dueDate: '2026-11-30',
        total: ExactAmount.parse('90.00'),
        paid: ExactAmount.zero,
        status: 'void',
        lines: const [],
      );
      expect(owedAcross([voided]).isZero, isTrue);
    });

    test('two spellings of one name are the same customer', () {
      final rows = registerOf([_customer('a', 'Acme Ltd')], const []);
      expect(nameAlreadyTaken('acme ltd ', rows)?.name, 'Acme Ltd');
      expect(nameAlreadyTaken('ACME  LTD', rows)?.name, 'Acme Ltd');
      expect(nameAlreadyTaken('Acme Limited', rows), isNull);
      expect(nameAlreadyTaken('   ', rows), isNull);
    });
  });

  group('the page', () {
    Widget page(InvoicingService service) =>
        MaterialApp(home: CustomerRegisterPage(service: service));

    testWidgets('leads with how many are on the books and how many owe', (
      tester,
    ) async {
      await tester.pumpWidget(
        page(
          _Served(
            customers: [_customer('a', 'Acme'), _customer('b', 'Bright')],
            invoices: [_invoice('INV-1', 'a', '300.00')],
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('2 customers, 1 owing'), findsOneWidget);
      expect(find.text('300.00'), findsOneWidget);
      expect(find.text('nothing owing'), findsOneWidget);
    });

    testWidgets('a customer never billed says so under their name', (
      tester,
    ) async {
      await tester.pumpWidget(
        page(_Served(customers: [_customer('a', 'Acme')], invoices: const [])),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('never billed'), findsOneWidget);
      expect(find.textContaining('1 customer, none owing'), findsOneWidget);
    });

    testWidgets('an empty register says nobody is on the books', (
      tester,
    ) async {
      await tester.pumpWidget(
        page(_Served(customers: const [], invoices: const [])),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('Nobody is on the books yet'), findsOneWidget);
      expect(find.text('Add a customer'), findsOneWidget);
    });

    testWidgets('a failed read is never an empty register', (tester) async {
      await tester.pumpWidget(page(_Fails()));
      await tester.pumpAndSettle();

      expect(find.byType(CouldNotRead), findsOneWidget);
      expect(find.textContaining('Nobody is on the books'), findsNothing);
    });

    testWidgets('a second spelling of a name is refused, naming the first', (
      tester,
    ) async {
      final service = _Served(
        customers: [_customer('a', 'Acme Ltd')],
        invoices: const [],
      );
      await tester.pumpWidget(page(service));
      await tester.pumpAndSettle();

      await tester.tap(find.byTooltip('Add a customer'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField).first, 'acme ltd');
      await tester.tap(find.text('Add'));
      await tester.pumpAndSettle();

      expect(find.textContaining('already on the books'), findsOneWidget);
      expect(service.created, isEmpty);
    });

    testWidgets('a customer with no invoices can be removed', (tester) async {
      final service = _Served(
        customers: [_customer('a', 'Acme')],
        invoices: const [],
      );
      await tester.pumpWidget(page(service));
      await tester.pumpAndSettle();

      await tester.tap(find.byTooltip('More for Acme'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Remove'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Remove'));
      await tester.pumpAndSettle();

      expect(service.removed, ['a']);
    });

    testWidgets(
      'a customer with an invoice is refused removal before sending',
      (tester) async {
        final service = _Served(
          customers: [_customer('a', 'Acme')],
          invoices: [_invoice('INV-1', 'a', '300.00')],
        );
        await tester.pumpWidget(page(service));
        await tester.pumpAndSettle();

        await tester.tap(find.byTooltip('More for Acme'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Remove'));
        await tester.pumpAndSettle();

        expect(find.textContaining('Cannot remove Acme'), findsOneWidget);
        expect(find.textContaining('1 invoice on file'), findsOneWidget);
        expect(service.removed, isEmpty);
      },
    );

    testWidgets('a misspelt name is corrected through the sheet', (
      tester,
    ) async {
      final service = _Served(
        customers: [_customer('a', 'Acne Ltd')],
        invoices: const [],
      );
      await tester.pumpWidget(page(service));
      await tester.pumpAndSettle();

      await tester.tap(find.byTooltip('More for Acne Ltd'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Change details'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField).first, 'Acme Ltd');
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();

      expect(service.updated, ['a:Acme Ltd']);
    });
  });
}
