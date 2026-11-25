// The tax ledger, and paying a tax over.
//
// Two things are pinned here. The reading, because each tax is its own account and a figure merged across them hides which
// authority is owed what. And the payment, because a tax recorded as owed that can never be shown as paid leaves a
// liability standing for ever: the screen must post the entry, say what it did, and refuse a figure larger than what is
// owed before anything is sent.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/tax/data/tax_ledger.dart';
import 'package:vimbai_mobile_client/features/tax/presentation/tax_ledger_page.dart';

const _october = '''
{"from":"2026-10-01","to":"2026-10-31",
 "taxes":[
   {"account_number":"2100","account_name":"VAT Payable","tax_name":"VAT","rate":"15.5000",
    "opening":"0.000000","charged":"150.000000","taken_back":"60.000000","paid":"0.000000",
    "returned":"0.000000","closing":"90.000000","is_reclaimable":false,
    "movements":[
      {"entry_id":"e1","entry_date":"2026-10-05","description":"Sale INV-1","reference":"INV-1",
       "source_module":"Invoicing","kind":"charged","debit":"0.000000","credit":"150.000000",
       "balance":"150.000000"},
      {"entry_id":"e2","entry_date":"2026-10-09","description":"Purchase PO-1","reference":"PO-1",
       "source_module":"Invoicing","kind":"taken_back","debit":"60.000000","credit":"0.000000",
       "balance":"90.000000"}]},
   {"account_number":"2110","account_name":"PAYE Payable","tax_name":"","rate":"",
    "opening":"0.000000","charged":"0.000000","taken_back":"0.000000","paid":"0.000000",
    "returned":"0.000000","closing":"0.000000","is_reclaimable":false,"movements":[]}],
 "account_count":2,
 "totals":{"opening":"0.000000","charged":"150.000000","taken_back":"60.000000","paid":"0.000000",
           "returned":"0.000000","closing":"90.000000"},
 "has_tax_account":true,
 "note":"Each tax is its own account, so what is owed is never one merged figure."}
''';

const _noTaxAccount = '''
{"from":"2026-10-01","to":"2026-10-31","taxes":[],"account_count":0,
 "totals":{"opening":"0.000000","charged":"0.000000","taken_back":"0.000000","paid":"0.000000",
           "returned":"0.000000","closing":"0.000000"},
 "has_tax_account":false,
 "note":"This Book's chart holds no tax account, so nothing has been posted as tax."}
''';

TaxLedgerService _serviceReturning(
  String body, {
  int status = 200,
  List<Map<String, dynamic>>? sent,
}) {
  final service = TaxLedgerService.fresh();
  service.useClient(
    MockClient((request) async {
      sent?.add({
        'method': request.method,
        'url': request.url.toString(),
        'body': request.body,
      });
      return http.Response(body, status, headers: const {});
    }),
  );
  service.setAuthToken('token');
  return service;
}

void main() {
  group('the ledger', () {
    test('each tax is read as its own account, with what it owes', () {
      final ledger = TaxLedger.fromJson(_jsonOf(_october));
      expect(ledger.hasTaxAccount, isTrue);
      expect(ledger.accounts, hasLength(2));

      final vat = ledger.accounts.first;
      expect(vat.label, 'VAT');
      expect(vat.place, '2100 VAT Payable');
      expect(vat.rateWords, '15.5%');
      expect(vat.owedWords, '90.00 owed');
      expect(vat.canBePaidOver, isTrue);
      expect(vat.movements, hasLength(2));
      expect(
        vat.movements.first.reading,
        'Charged 150.00 on 2026-10-05, INV-1',
      );
      expect(
        vat.movements.last.reading,
        'Taken back 60.00 on 2026-10-09, PO-1',
      );

      // an account the chart holds with nothing on it is still named, and nothing is owed on it
      final paye = ledger.accounts.last;
      expect(paye.label, 'PAYE Payable');
      expect(paye.owedWords, 'Nothing owed');
      expect(paye.canBePaidOver, isFalse);
      expect(paye.moved, 'Nothing moved in this period');
    });

    test(
      'what the period did is said in words, leaving out what did not happen',
      () {
        final vat = TaxLedger.fromJson(_jsonOf(_october)).accounts.first;
        expect(vat.moved, '150.00 charged, 60.00 taken back');
        expect(
          TaxLedger.fromJson(_jsonOf(_october)).headline,
          '90.00 owed in tax',
        );
      },
    );

    test('a tax with nothing owed on it is not offered for payment', () {
      final ledger = TaxLedger.fromJson(_jsonOf(_october));
      final settled = TaxLedgerAccount.fromJson({
        'account_number': '2100',
        'account_name': 'VAT Payable',
        'opening': '0',
        'charged': '150.000000',
        'taken_back': '0',
        'paid': '150.000000',
        'returned': '0',
        'closing': '0.000000',
        'is_reclaimable': false,
        'movements': const [],
      });
      expect(settled.canBePaidOver, isFalse);
      expect(settled.owedWords, 'Nothing owed');
      expect(ledger.detail, contains('between 2026-10-01 and 2026-10-31'));
    });

    test(
      'a book with no tax account says what to add rather than nothing owed',
      () {
        final ledger = TaxLedger.fromJson(_jsonOf(_noTaxAccount));
        expect(ledger.hasTaxAccount, isFalse);
        expect(ledger.headline, 'This Book has no tax account');
        expect(ledger.detail, contains('Chart of accounts'));
      },
    );

    test('a tax owed the business is read as money to reclaim', () {
      final account = TaxLedgerAccount.fromJson({
        'account_number': '2100',
        'account_name': 'VAT Payable',
        'closing': '-300.000000',
        'is_reclaimable': true,
        'movements': const [],
      });
      expect(account.owedWords, '300.00 to reclaim');
      expect(account.canBePaidOver, isFalse);
    });
  });

  group('paying a tax over', () {
    test('the payment is sent to the ledger with the day and the figure', () async {
      final sent = <Map<String, dynamic>>[];
      final service = _serviceReturning(
        '{"entry_date":"2026-11-05","amount":"90.000000","tax_account_number":"2100",'
        '"tax_name":"VAT","money_account_number":"1010","money_account_name":"Bank",'
        '"reference":"TAX-2026-11-05-2100-90.000000","owed_after":"0.000000"}',
        sent: sent,
      );
      final paid = await service.payOver(
        entryDate: '2026-11-05',
        taxAccountNumber: '2100',
        amount: '90.00',
        period: '2026-10',
      );
      expect(paid.taxName, 'VAT');
      expect(paid.moneyAccountName, 'Bank');
      expect(paid.owedAfter.isZero, isTrue);
      expect(
        paid.reading,
        '90.00 of VAT paid out of Bank on 2026-11-05. Nothing is owed on it now.',
      );
      expect(sent.single['method'], 'POST');
      expect(sent.single['url'], endsWith('/accounting/tax-ledger/settle'));
      expect(sent.single['body'], contains('"amount":"90.00"'));
      expect(sent.single['body'], contains('"period":"2026-10"'));
    });

    test('the engine\'s refusal is what the person is told', () async {
      final service = _serviceReturning(
        '{"error":"A payment of 200.00 is more than the 90.00 owed on VAT Payable."}',
        status: 422,
      );
      await expectLater(
        service.payOver(
          entryDate: '2026-11-05',
          taxAccountNumber: '2100',
          amount: '200.00',
        ),
        throwsA(
          predicate(
            (error) => error.toString().contains(
              'more than the 90.00 owed on VAT Payable',
            ),
          ),
        ),
      );
    });
  });

  group('the screen', () {
    testWidgets('states each tax, what it moved and what is owed', (
      tester,
    ) async {
      await tester.pumpWidget(
        MaterialApp(
          home: TaxLedgerPage(
            service: _serviceReturning(_october),
            today: '2026-10-20',
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('90.00 owed in tax'), findsOneWidget);
      expect(find.text('VAT'), findsOneWidget);
      expect(find.text('2100 VAT Payable · 15.5%'), findsOneWidget);
      expect(find.text('90.00 owed'), findsOneWidget);
      expect(find.text('150.00 charged, 60.00 taken back'), findsOneWidget);
      expect(find.text('PAYE Payable'), findsOneWidget);
      expect(find.text('Nothing owed'), findsOneWidget);
      // only the tax that is owed is offered for payment
      expect(find.text('Pay it over'), findsOneWidget);

      // the movements behind the figure are one tap away
      await tester.tap(find.text('What moved it').first);
      await tester.pumpAndSettle();
      expect(find.text('Charged 150.00 on 2026-10-05, INV-1'), findsOneWidget);
      expect(find.text('Taken back 60.00 on 2026-10-09, PO-1'), findsOneWidget);
    });

    testWidgets('a book with no tax account is told what to add', (
      tester,
    ) async {
      await tester.pumpWidget(
        MaterialApp(
          home: TaxLedgerPage(
            service: _serviceReturning(_noTaxAccount),
            today: '2026-10-20',
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('This Book has no tax account'), findsOneWidget);
      expect(find.text('Add a tax account'), findsOneWidget);
      expect(find.text('Pay it over'), findsNothing);
    });

    testWidgets('paying over posts the figure that is owed', (tester) async {
      final sent = <Map<String, dynamic>>[];
      final service = _serviceReturning(_october, sent: sent);
      await tester.pumpWidget(
        MaterialApp(
          home: TaxLedgerPage(service: service, today: '2026-10-20'),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Pay it over'));
      await tester.pumpAndSettle();
      // the sheet leads with what the books say, so the figure is not a guess
      expect(find.text('Pay VAT over'), findsOneWidget);
      expect(find.text('Pay it over'), findsNWidgets(2));
      expect(
        find.textContaining('90.00 owed on 2100 VAT Payable'),
        findsOneWidget,
      );
    });

    testWidgets('a payment larger than what is owed stops before it is sent', (
      tester,
    ) async {
      final sent = <Map<String, dynamic>>[];
      final service = _serviceReturning(_october, sent: sent);
      await tester.pumpWidget(
        MaterialApp(
          home: TaxLedgerPage(service: service, today: '2026-10-20'),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Pay it over'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField).last, '200.00');
      await tester.tap(find.widgetWithText(FilledButton, 'Pay it over'));
      await tester.pumpAndSettle();

      expect(find.textContaining('more than the 90.00 owed'), findsOneWidget);
      // nothing was sent: only the read that opened the screen reached the engine
      expect(sent.where((call) => call['method'] == 'POST'), isEmpty);
    });
  });
}

Map<String, dynamic> _jsonOf(String body) =>
    jsonDecode(body) as Map<String, dynamic>;
