// What a sale owes the taxman, and the screen that reads it back from the Book.
//
// Two things are pinned here. The split, because a taxed sale posted as one credit overstates income by the tax and leaves
// the liability off the balance sheet entirely, and the refusals, because a tax larger than the invoice is a total typed
// into the wrong field and must stop before anything is sent. The screen, because a nil figure with no VAT account behind
// it looks like good news when it is really a missing account.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/tax/data/sale_tax.dart';
import 'package:vimbai_mobile_client/features/tax/data/tax_due.dart';
import 'package:vimbai_mobile_client/features/tax/presentation/what_you_owe_page.dart';

const _owed = PostingAccount(
  accountNumber: '1100',
  accountName: 'Accounts Receivable',
);
const _earned = PostingAccount(accountNumber: '4000', accountName: 'Sales');
const _vat = PostingAccount(accountNumber: '2100', accountName: 'VAT Payable');

SaleSplit _split({required String total, required String tax}) => saleSplit(
  owed: _owed,
  earned: _earned,
  taxAccount: _vat,
  total: ExactAmount.parse(total),
  tax: ExactAmount.parse(tax),
);

TaxDueService _serviceReturning(String body, {int status = 200}) {
  final service = TaxDueService.fresh();
  service.useClient(
    MockClient(
      (request) async => http.Response(body, status, headers: const {}),
    ),
  );
  service.setAuthToken('token');
  return service;
}

const _october = '''
{"from":"2026-10-01","to":"2026-10-31","collected":"150.000000","reclaimable":"60.000000",
 "due":"90.000000",
 "accounts":[{"account_number":"2100","name":"VAT Payable","collected":"150.000000",
              "reclaimable":"60.000000","due":"90.000000"}],
 "documents":[{"kind":"purchase","number":"PO-1","date":"2026-10-09","total":"460.000000",
               "tax":"60.000000"},
              {"kind":"sale","number":"INV-1","date":"2026-10-05","total":"1150.000000",
               "tax":"150.000000"}],
 "has_tax_account":true}
''';

void main() {
  group('the split', () {
    test(
      'a taxed sale earns the invoice less its tax, and the tax sits apart',
      () {
        final split = _split(total: '1150', tax: '150');
        expect(split.revenue, ExactAmount.parse('1000'));
        expect(split.carriesTax, isTrue);

        final lines = split.linesFor('INV-1');
        expect(lines.length, 3);
        expect(lines[0].accountNumber, '1100');
        expect(lines[0].debit, ExactAmount.parse('1150'));
        expect(lines[1].accountNumber, '4000');
        expect(lines[1].credit, ExactAmount.parse('1000'));
        expect(lines[2].accountNumber, '2100');
        expect(lines[2].credit, ExactAmount.parse('150'));
        // the two credits still add to the debit, so the entry balances as it always did
        expect(lines[1].credit + lines[2].credit, lines[0].debit);
        expect(lines[2].description, 'Tax on INV-1');
      },
    );

    test('an untaxed sale posts two lines, exactly as before', () {
      final lines = _split(total: '460', tax: '0').linesFor('INV-2');
      expect(lines.length, 2);
      expect(lines[1].credit, ExactAmount.parse('460'));
    });

    test('a tax larger than the invoice is refused naming both figures', () {
      expect(
        () => _split(total: '100', tax: '150'),
        throwsA(
          predicate(
            (error) =>
                error.toString().contains('150') &&
                error.toString().contains('100'),
          ),
        ),
      );
    });

    test(
      'a tax equal to the whole invoice is refused, because the sale earns nothing',
      () {
        expect(
          () => _split(total: '100', tax: '100'),
          throwsA(
            predicate(
              (error) => error.toString().contains('carry nothing but tax'),
            ),
          ),
        );
      },
    );

    test('the reading shows the three sides before anything is sent', () {
      expect(_split(total: '1150', tax: '150').reading, [
        'Dr 1100 Accounts Receivable  1150.00',
        'Cr 4000 Sales  1000.00',
        'Cr 2100 VAT Payable  150.00',
      ]);
    });
  });

  group('the month a return covers', () {
    test('the window runs from the first to the last day of the month', () {
      expect(monthWindow('2026-10-05'), (from: '2026-10-01', to: '2026-10-31'));
      expect(monthWindow('2026-02-11'), (from: '2026-02-01', to: '2026-02-28'));
      expect(monthWindow('2024-02-11'), (from: '2024-02-01', to: '2024-02-29'));
      expect(monthWindow('2026-12-31'), (from: '2026-12-01', to: '2026-12-31'));
    });

    test(
      'something that is not a date gives no window rather than a wrong one',
      () {
        expect(monthWindow(''), (from: '', to: ''));
        expect(monthWindow('october'), (from: '', to: ''));
        expect(monthWindow('2026-13-01'), (from: '', to: ''));
      },
    );
  });

  group('the reading', () {
    test('what was collected, what was reclaimed, and what is due', () {
      final tax = TaxDue.fromJson(
        (const {
              'from': '2026-10-01',
              'to': '2026-10-31',
              'collected': '150.000000',
              'reclaimable': '60.000000',
              'due': '90.000000',
              'accounts': [
                {
                  'account_number': '2100',
                  'name': 'VAT Payable',
                  'collected': '150.000000',
                  'reclaimable': '60.000000',
                },
              ],
              'documents': [
                {
                  'kind': 'sale',
                  'number': 'INV-1',
                  'date': '2026-10-05',
                  'total': '1150.000000',
                  'tax': '150.000000',
                },
              ],
              'has_tax_account': true,
            })
            as Map<String, dynamic>,
      );
      expect(tax.headline, '90.00 owed in tax');
      expect(tax.detail, contains('150.00 collected'));
      expect(tax.detail, contains('60.00 reclaimed'));
      expect(tax.reclaiming, isFalse);
      expect(tax.accounts.single.label, '2100 VAT Payable');
      expect(tax.documents.single.reading, 'Sale INV-1, 2026-10-05');
      expect(tax.documents.single.isSale, isTrue);
    });

    test(
      'a period that reclaims more than it collected reads as money coming back',
      () {
        final tax = TaxDue.fromJson({
          'from': '2026-10-01',
          'to': '2026-10-31',
          'collected': '0.000000',
          'reclaimable': '300.000000',
          'due': '-300.000000',
          'accounts': const [],
          'documents': const [],
          'has_tax_account': true,
        });
        expect(tax.reclaiming, isTrue);
        expect(tax.headline, '300.00 to reclaim');
      },
    );

    test(
      'a Book with no VAT account says what to add instead of reporting a nil',
      () {
        final tax = TaxDue.fromJson({
          'from': '2026-10-01',
          'to': '2026-10-31',
          'collected': '0.000000',
          'reclaimable': '0.000000',
          'due': '0.000000',
          'accounts': const [],
          'documents': const [],
          'has_tax_account': false,
        });
        expect(tax.headline, 'This Book has no VAT account');
        expect(tax.detail, contains('Chart of accounts'));
      },
    );

    test('a period with no taxed trade says nothing was charged', () {
      final tax = TaxDue.fromJson({
        'from': '2026-11-01',
        'to': '2026-11-30',
        'collected': '0.000000',
        'reclaimable': '0.000000',
        'due': '0.000000',
        'accounts': const [],
        'documents': const [],
        'has_tax_account': true,
      });
      expect(tax.headline, 'No tax was charged in this period');
      expect(tax.nothingTaxed, isTrue);
    });
  });

  group('the screen', () {
    testWidgets(
      'it leads with what is owed and names the documents behind it',
      (tester) async {
        await tester.pumpWidget(
          MaterialApp(
            home: WhatYouOwePage(
              service: _serviceReturning(_october),
              today: '2026-10-20',
            ),
          ),
        );
        await tester.pumpAndSettle();

        expect(find.text('90.00 owed in tax'), findsOneWidget);
        expect(find.textContaining('150.00 collected'), findsWidgets);
        expect(find.text('2100 VAT Payable'), findsOneWidget);
        expect(find.text('Sale INV-1, 2026-10-05'), findsOneWidget);
        expect(find.text('Purchase PO-1, 2026-10-09'), findsOneWidget);
      },
    );

    testWidgets('a Book with no VAT account is told what to add', (
      tester,
    ) async {
      await tester.pumpWidget(
        MaterialApp(
          home: WhatYouOwePage(
            service: _serviceReturning(
              '{"from":"2026-10-01","to":"2026-10-31","collected":"0.000000",'
              '"reclaimable":"0.000000","due":"0.000000","accounts":[],'
              '"documents":[],"has_tax_account":false}',
            ),
            today: '2026-10-20',
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('This Book has no VAT account'), findsOneWidget);
      expect(find.text('Add a VAT account'), findsOneWidget);
    });

    testWidgets('a failed read is not shown as nothing owed', (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          home: WhatYouOwePage(
            service: _serviceReturning('{"error":"no book"}', status: 400),
            today: '2026-10-20',
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byType(CouldNotRead), findsOneWidget);
      expect(find.textContaining('owed in tax'), findsNothing);
      expect(find.text('No tax was charged in this period'), findsNothing);
    });
  });
}
