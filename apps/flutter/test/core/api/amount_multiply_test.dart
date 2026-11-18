// A quantity times a unit price is an amount, exactly.
//
// The app multiplies two six-place figures for every invoice line, so the product is a real arithmetic result rather than
// a float: three of something at 250.00 is 750.00, and the awkward cases below are the ones a float gets wrong. The
// carrying back to six places rounds once, half to even, which is the rule the engine's own division uses - a line the
// phone works out and the same line the engine works out have to agree, or an invoice total that is right on the phone is
// refused by the ledger.
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';

/// Amounts compare exactly, so the assertions compare amounts rather than the two-place display text.

ExactAmount _of(String text) => ExactAmount.parse(text);

void main() {
  test('whole quantities land on the figure a person would write', () {
    expect((_of('250.00') * _of('3')), _of('750.00'));
    expect((_of('19.99') * _of('12')), _of('239.88'));
    expect((_of('0.01') * _of('7')), _of('0.07'));
  });

  test(
    'a fractional quantity keeps its share instead of rounding to nothing',
    () {
      expect((_of('100.00') * _of('0.5')), _of('50.00'));
      expect((_of('33.33') * _of('1.5')), _of('49.995'));
      expect((_of('0.10') * _of('0.333333')), _of('0.033333'));
    },
  );

  test('the product is carried to six places by rounding once, half to even', () {
    // 0.0000015 is exactly half way between 0.000001 and 0.000002; half to even takes the even last digit
    expect((_of('0.000003') * _of('0.5')), _of('0.000002'));
    expect((_of('0.000001') * _of('0.5')), ExactAmount.zero);
    // just past half way rounds up, just short of it rounds down
    expect((_of('0.000003') * _of('0.500001')), _of('0.000002'));
    expect((_of('0.000003') * _of('0.499999')), _of('0.000001'));
  });

  test(
    'a very large quantity keeps every digit rather than losing the top of the number',
    () {
      final huge = _of('123456789012345678901234567890.000001');
      expect(huge * _of('2'), _of('246913578024691357802469135780.000002'));
    },
  );

  test('direction survives, and nothing times nothing is nothing', () {
    expect((_of('-250.00') * _of('3')), _of('-750.00'));
    expect((_of('-2') * _of('-3')), _of('6'));
    expect((_of('250.00') * ExactAmount.zero), ExactAmount.zero);
    expect((_of('250.00') * ExactAmount.zero).isZero, isTrue);
  });
}
