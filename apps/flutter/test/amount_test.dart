import 'package:flutter_test/flutter_test.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';

void main() {
  group('reading money the engine sends', () {
    test('a decimal string is read rather than thrown at', () {
      // the engine answers "250.000000": a double cast on that string is what broke the journal screen
      expect(amountFromJson('250.000000'), 250.0);
      expect(amountFromJson('0.000000'), 0.0);
      expect(amountFromJson('-12.500000'), -12.5);
    });

    test('a number is still accepted, so both forms work', () {
      expect(amountFromJson(250), 250.0);
      expect(amountFromJson(250.5), 250.5);
    });

    test('nothing at all reads as nothing, never a crash', () {
      expect(amountFromJson(null), 0.0);
      expect(amountFromJson(''), 0.0);
      expect(amountFromJson('not a number'), 0.0);
      expect(amountFromJson(true), 0.0);
    });
  });
}
