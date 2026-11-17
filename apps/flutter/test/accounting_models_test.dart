import 'package:flutter_test/flutter_test.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';

/// An entry exactly as the engine answers one: money as a decimal string, lines keyed `account_number`.
final engineReply = {
  'id': '18e84bfc4dd9eea2',
  'entry_date': '2026-11-15T20:41:00.123456',
  'description': 'first sale from the phone',
  'reference_number': 'INV-1',
  'source_module': 'MobileApp',
  'status': 'draft',
  'created_at': '2026-11-15T18:39:11Z',
  'updated_at': '2026-11-15T18:39:11Z',
  'lines': [
    {
      'account_number': '1000',
      'debit': '250.000000',
      'credit': '0.000000',
      'description': '',
    },
    {
      'account_number': '4000',
      'debit': '0.000000',
      'credit': '250.000000',
      'description': '',
    },
  ],
};

void main() {
  group('reading an entry the engine answered', () {
    test('a posted entry parses, lines and money included', () {
      final entry = JournalEntry.fromJson(engineReply);

      expect(entry.id, '18e84bfc4dd9eea2');
      expect(entry.status, 'draft');
      expect(entry.lines, hasLength(2));
      expect(entry.lines.first.accountNumber, '1000');
      expect(entry.lines.first.debit, ExactAmount.parse('250'));
      expect(entry.lines.last.credit, ExactAmount.parse('250'));
    });

    test(
      'a local row parses the same way, because the table column is the same name',
      () {
        // database_helper reads each journal_lines row straight into this model
        final line = JournalLine.fromJson({
          'account_number': '4000',
          'debit': 0.0,
          'credit': 250.0,
          'description': null,
        });

        expect(line.accountNumber, '4000');
        expect(line.credit, ExactAmount.parse('250'));
      },
    );

    test('what is read can be written back out under the same names', () {
      final line = JournalLine(
        accountNumber: '1000',
        debit: ExactAmount.parse('250.00'),
      );
      expect(line.toJson()['account_number'], '1000');
      expect(line.toJson().containsKey('accountNumber'), isFalse);
    });
  });

  group('reading a trial balance the engine answered', () {
    test('the rows arrive under the name the engine writes', () {
      final trial = TrialBalance.fromJson({
        'report_date': '2026-12-31T00:00:00.000',
        'accounts': [
          {
            'account_number': '1000',
            'account_name': 'Cash',
            'account_type': 'asset',
            'debit': '250.000000',
            'credit': '0.000000',
          },
          {
            'account_number': '4000',
            'account_name': 'Sales',
            'account_type': 'revenue',
            'debit': '0.000000',
            'credit': '250.000000',
          },
        ],
        'total_debits': '250.000000',
        'total_credits': '250.000000',
        'is_balanced': true,
      });

      expect(trial.entries, hasLength(2));
      expect(trial.entries.first.accountNumber, '1000');
      // money comes back exactly: the digits the engine wrote, not a double that happened to hold them
      expect(trial.entries.last.creditTotal, ExactAmount.parse('250.000000'));
      expect(trial.totalDebits, ExactAmount.parse('250.000000'));
      expect(trial.totalDebits, trial.totalCredits);
      expect(trial.isBalanced, isTrue);
    });
  });
}
