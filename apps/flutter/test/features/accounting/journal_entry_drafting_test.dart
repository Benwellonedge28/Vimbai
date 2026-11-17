import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/models/journal_entry_drafting.dart';

JournalLine line(String account, int debit, int credit) => JournalLine(
  accountNumber: account,
  debit: ExactAmount.units(debit),
  credit: ExactAmount.units(credit),
);

void main() {
  final lines = [line('1001', 25, 0), line('4000', 0, 25)];

  test('a submitted entry carries the status that makes it count', () {
    final entry = buildJournalEntry(
      id: 'e1',
      entryDate: DateTime.utc(2026, 3, 1),
      description: 'Consulting income',
      lines: lines,
    );
    // The engine keeps an entry with no status as a draft and leaves drafts out of the statements, so this is the
    // difference between a phone that posts and a phone that only looks like it does.
    expect(entry.status, 'posted');
    expect(entry.toJson()['status'], 'posted');
  });

  test('a kept draft is the same entry, held out of the statements', () {
    final draft = buildJournalEntry(
      id: 'e2',
      entryDate: DateTime.utc(2026, 3, 1),
      description: 'Consulting income',
      lines: lines,
      intent: JournalEntryIntent.keepAsDraft,
    );
    expect(draft.status, 'draft');
    expect(draft.toJson()['status'], 'draft');
    // nothing else differs, so posting the same work later says what was written
    expect(draft.description, 'Consulting income');
    expect(draft.lines.length, 2);
  });

  test('the list calls an entry a draft exactly when the books do', () {
    JournalEntry withStatus(String? status) => JournalEntry(
      id: 's',
      entryDate: DateTime.utc(2026, 3, 1),
      description: 'Anything',
      sourceModule: 'MobileApp',
      status: status,
      lines: lines,
    );
    expect(isHeldAsDraft(withStatus('draft')), isTrue);
    // an entry that arrives with no status is a draft to the engine, so the list must not imply otherwise
    expect(isHeldAsDraft(withStatus(null)), isTrue);
    expect(isHeldAsDraft(withStatus('posted')), isFalse);
  });

  test('a blank reference is sent as nothing rather than an empty string', () {
    final entry = buildJournalEntry(
      id: 'e3',
      entryDate: DateTime.utc(2026, 3, 1),
      description: 'Cash sale',
      lines: lines,
      referenceNumber: '   ',
    );
    expect(entry.referenceNumber, isNull);
  });

  test('a written reference is kept, trimmed', () {
    final entry = buildJournalEntry(
      id: 'e4',
      entryDate: DateTime.utc(2026, 3, 1),
      description: 'Cash sale',
      lines: lines,
      referenceNumber: '  INV-9 ',
    );
    expect(entry.referenceNumber, 'INV-9');
  });
}
