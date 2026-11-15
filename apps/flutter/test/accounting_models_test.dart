import 'package:flutter_test/flutter_test.dart';

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
    {'account_number': '1000', 'debit': '250.000000', 'credit': '0.000000', 'description': ''},
    {'account_number': '4000', 'debit': '0.000000', 'credit': '250.000000', 'description': ''},
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
      expect(entry.lines.first.debit, 250.0);
      expect(entry.lines.last.credit, 250.0);
    });

    test('a local row parses the same way, because the table column is the same name', () {
      // database_helper reads each journal_lines row straight into this model
      final line = JournalLine.fromJson({
        'account_number': '4000',
        'debit': 0.0,
        'credit': 250.0,
        'description': null,
      });

      expect(line.accountNumber, '4000');
      expect(line.credit, 250.0);
    });

    test('what is read can be written back out under the same names', () {
      final line = JournalLine(accountNumber: '1000', debit: 250.0);
      expect(line.toJson()['account_number'], '1000');
      expect(line.toJson().containsKey('accountNumber'), isFalse);
    });
  });
}
