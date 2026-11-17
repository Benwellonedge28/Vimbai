import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';

void main() {
  test('reads the shape the ledger stores', () {
    expect(ExactAmount.parse('250.000000').toString(), '250.00');
    expect(ExactAmount.parse('0.000001').toString(), '0.000001');
    expect(ExactAmount.parse('-3.5').toString(), '-3.50');
    expect(ExactAmount.parse('007').toString(), '7.00');
    expect(ExactAmount.parse('.5').toString(), '0.50');
  });

  test('refuses what a ledger would have to round', () {
    // rounding an amount on the way in is how a person's figure becomes somebody else's figure
    expect(ExactAmount.tryParse('1.0000001'), isNull);
    expect(ExactAmount.tryParse('1,000'), isNull);
    expect(ExactAmount.tryParse('1e9'), isNull);
    expect(ExactAmount.tryParse('abc'), isNull);
    expect(ExactAmount.tryParse(''), isNull);
    expect(ExactAmount.tryParse('1.0000000'), ExactAmount.parse('1'));
  });

  test('adds without losing a digit a double would lose', () {
    // 0.1 + 0.2 is the classic double failure; a ledger cannot have one
    final total = ExactAmount.parse('0.1') + ExactAmount.parse('0.2');
    expect(total, ExactAmount.parse('0.3'));
    expect(total.toLedgerString(), '0.300000');

    // a 30-digit figure: a double holds 15 significant digits and would round the tail away
    final huge = ExactAmount.parse('123456789012345678901234567890.123456');
    expect(huge.toLedgerString(), '123456789012345678901234567890.123456');
    expect(
      huge + ExactAmount.parse('0.000001'),
      ExactAmount.parse('123456789012345678901234567890.123457'),
    );
    expect(huge - huge, ExactAmount.zero);
  });

  test('a balanced pair balances exactly at any size', () {
    final debit = ExactAmount.parse('999999999999999999999999999999.999999');
    final credit =
        ExactAmount.parse('1000000000000000000000000000000') -
        ExactAmount.parse('0.000001');
    expect(debit, credit);
    expect(debit - credit, ExactAmount.zero);
  });

  test('one minor unit out is not balance, however big the amounts', () {
    final debit = ExactAmount.parse('1000000000000000000000000000000.000001');
    final credit = ExactAmount.parse('1000000000000000000000000000000.000000');
    expect(debit, isNot(credit));
    expect((debit - credit).toLedgerString(), '0.000001');
  });

  test('subtracts across zero and keeps the sign right', () {
    expect(
      ExactAmount.parse('3') - ExactAmount.parse('5'),
      ExactAmount.parse('-2'),
    );
    expect(
      ExactAmount.parse('5') - ExactAmount.parse('3'),
      ExactAmount.parse('2'),
    );
    expect(
      ExactAmount.parse('-3') - ExactAmount.parse('-5'),
      ExactAmount.parse('2'),
    );
    expect(ExactAmount.parse('-3') + ExactAmount.parse('3'), ExactAmount.zero);
    expect(ExactAmount.parse('-3').compareTo(ExactAmount.parse('3')), -1);
    expect(ExactAmount.parse('3').compareTo(ExactAmount.parse('-3')), 1);
  });

  test('json keeps the digits in both directions', () {
    const json = '123456789012345678901234567890.123456';
    expect(exactAmountToJson(ExactAmount.parse(json)), json);
    expect(exactAmountFromJson(json), ExactAmount.parse(json));
    expect(exactAmountFromJson(25.5), ExactAmount.parse('25.5'));
    expect(exactAmountFromJson(null), ExactAmount.zero);
  });

  test('sums a hundred thousand lines exactly', () {
    final total = ExactAmount.sum(
      List.generate(100000, (_) => ExactAmount.parse('1234.567891')),
    );
    expect(total, ExactAmount.parse('123456789.100000'));
  });
}
