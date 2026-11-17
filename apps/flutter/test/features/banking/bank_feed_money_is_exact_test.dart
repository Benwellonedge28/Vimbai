// A bank feed row carries the amount as a decimal string. Read through a double, a balance past fifteen digits came
// back rewritten, and printing it to two places threw away the rest of the figure.
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/banking/models/banking_models.dart';

void main() {
  test('a feed row keeps every digit of its amount and running balance', () {
    const amount = '123456789012345678901234567890.123456';
    final row = BankTransaction.fromJson({
      'id': 't1',
      'bank_connection_id': 'c1',
      'amount': amount,
      'transaction_type': 'credit',
      'description': 'Bulk transfer',
      'date': '2026-11-17',
      'balance_after': '999999999999999999999999999999.000001',
      'status': 'reconciled',
    });

    expect(row.amount, ExactAmount.parse(amount));
    expect('${row.amount}', amount);
    expect('${row.balanceAfter}', '999999999999999999999999999999.000001');
    expect(row.transactionType, 'credit');
    expect(row.reconciled, isTrue);
  });

  test(
    'a figure that is not a number is read as nothing rather than throwing',
    () {
      final row = BankTransaction.fromJson({
        'id': 't2',
        'amount': 'not a figure',
        'date': '2026-11-17',
      });
      expect(row.amount, ExactAmount.zero);
    },
  );
}
