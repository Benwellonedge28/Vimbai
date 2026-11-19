// What a bill does to the books, before anything is written.
//
// These are the ordinary double entries, and the tests pin them because getting the direction wrong is the kind of mistake
// that makes a set of books quietly wrong rather than visibly broken: a bill that debited the payable account would show
// the supplier being owed less the more you bought from them.
//
// The refusals matter as much. A chart with no payable account, or with nowhere to put the cost, is a case where posting
// something plausible would be worse than posting nothing.
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/payables/data/bill_posting.dart';

Account _account(String number, String name, String type) => Account(
  id: 'acc-$number',
  accountNumber: number,
  accountName: name,
  accountType: type,
  normalBalance: type == 'asset' || type == 'expense' ? 'debit' : 'credit',
);

List<Account> _chart() => [
  _account('1000', 'Cash', 'asset'),
  _account('1010', 'Bank', 'asset'),
  _account('2000', 'Accounts Payable', 'liability'),
  _account('5000', 'Purchases', 'expense'),
  _account('5100', 'Rent', 'expense'),
];

void main() {
  test('a bill is charged to the cost and owed to the supplier', () {
    final posting = billPosting(
      chart: _chart(),
      amount: ExactAmount.parse('480'),
      supplierName: 'Paper Co',
      reference: 'INV-9',
    );
    expect(posting.canPost, isTrue);
    expect(posting.reference, 'INV-9');
    expect(posting.sides.first.accountName, 'Purchases');
    expect(posting.sides.first.debit.toDecimalString(), '480.00');
    expect(posting.sides.first.credit.isZero, isTrue);
    expect(posting.sides.last.accountName, 'Accounts Payable');
    expect(posting.sides.last.credit.toDecimalString(), '480.00');
    expect(posting.sides.last.debit.isZero, isTrue);
    expect(posting.total.toDecimalString(), '480.00');
  });

  test(
    'the cost may be chosen, so a bill for rent is not posted to purchases',
    () {
      final chart = _chart();
      final rent = chart.firstWhere((a) => a.accountName == 'Rent');
      final posting = billPosting(
        chart: chart,
        amount: ExactAmount.parse('900'),
        supplierName: 'Rent Co',
        reference: 'LEASE-11',
        charged: rent,
      );
      expect(posting.sides.first.accountName, 'Rent');
      expect(posting.sides.last.accountName, 'Accounts Payable');
    },
  );

  test('a chart with no payable account is refused, naming the fix', () {
    final chart = [
      _account('1000', 'Cash', 'asset'),
      _account('5000', 'Purchases', 'expense'),
    ];
    final posting = billPosting(
      chart: chart,
      amount: ExactAmount.parse('100'),
      supplierName: 'Paper Co',
      reference: 'INV-1',
    );
    expect(posting.canPost, isFalse);
    expect(posting.refusal, contains('Accounts Payable'));
    expect(posting.refusal, contains('Chart of accounts'));
  });

  test('a chart with nowhere to put the cost is refused too', () {
    final chart = [
      _account('1000', 'Cash', 'asset'),
      _account('2000', 'Accounts Payable', 'liability'),
    ];
    final posting = billPosting(
      chart: chart,
      amount: ExactAmount.parse('100'),
      supplierName: 'Paper Co',
      reference: 'INV-1',
    );
    expect(posting.canPost, isFalse);
    expect(posting.refusal, contains('cost or expense'));
  });

  test('a bill of nothing is refused rather than posted as a zero entry', () {
    final posting = billPosting(
      chart: _chart(),
      amount: ExactAmount.zero,
      supplierName: 'Paper Co',
      reference: 'INV-0',
    );
    expect(posting.canPost, isFalse);
    expect(posting.refusal, contains('nothing'));
  });

  test('a supplier with no name still reads as somebody', () {
    final posting = billPosting(
      chart: _chart(),
      amount: ExactAmount.parse('10'),
      supplierName: '   ',
      reference: 'INV-2',
    );
    expect(posting.description, 'Bill from a supplier');
  });

  group('a bill that carries tax', () {
    List<Account> taxedChart() => [
      ..._chart(),
      _account('2100', 'VAT Payable', 'liability'),
    ];

    test('puts the tax on its own side, so the cost is what was bought', () {
      final posting = billPosting(
        chart: taxedChart(),
        amount: ExactAmount.parse('460'),
        supplierName: 'Paper Co',
        reference: 'INV-9',
        tax: ExactAmount.parse('60'),
      );
      expect(posting.canPost, isTrue);
      expect(posting.sides.length, 3);
      // what it cost, what can be reclaimed, and what is owed: the three still balance
      expect(posting.sides[0].accountName, 'Purchases');
      expect(posting.sides[0].debit.toDecimalString(), '400.00');
      expect(posting.sides[1].accountName, 'VAT Payable');
      expect(posting.sides[1].debit.toDecimalString(), '60.00');
      expect(posting.sides[2].accountName, 'Accounts Payable');
      expect(posting.sides[2].credit.toDecimalString(), '460.00');
      final debits = posting.sides
          .map((side) => side.debit)
          .fold(ExactAmount.zero, (sum, one) => sum + one);
      expect(debits, posting.sides.last.credit);
    });

    test('a bill with no tax posts the two sides it always did', () {
      final posting = billPosting(
        chart: taxedChart(),
        amount: ExactAmount.parse('460'),
        supplierName: 'Paper Co',
        reference: 'INV-9',
      );
      expect(posting.sides.length, 2);
      expect(posting.sides.first.debit.toDecimalString(), '460.00');
    });

    test(
      'a Book with no tax account refuses rather than losing the reclaim in the cost',
      () {
        final posting = billPosting(
          chart: _chart(),
          amount: ExactAmount.parse('460'),
          supplierName: 'Paper Co',
          reference: 'INV-9',
          tax: ExactAmount.parse('60'),
        );
        expect(posting.canPost, isFalse);
        expect(posting.refusal, contains('VAT account'));
        expect(posting.sides, isEmpty);
      },
    );

    test(
      'tax larger than the bill, or all of it, is refused naming the figures',
      () {
        final tooMuch = billPosting(
          chart: taxedChart(),
          amount: ExactAmount.parse('100'),
          supplierName: 'Paper Co',
          reference: 'INV-9',
          tax: ExactAmount.parse('150'),
        );
        expect(tooMuch.canPost, isFalse);
        expect(tooMuch.refusal, contains('150'));
        expect(tooMuch.refusal, contains('100'));

        final allOfIt = billPosting(
          chart: taxedChart(),
          amount: ExactAmount.parse('100'),
          supplierName: 'Paper Co',
          reference: 'INV-9',
          tax: ExactAmount.parse('100'),
        );
        expect(allOfIt.canPost, isFalse);
        expect(allOfIt.refusal, contains('carry nothing'));
      },
    );
  });
}
