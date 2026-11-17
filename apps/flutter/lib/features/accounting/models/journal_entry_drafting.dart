import 'accounting_models.dart';

/// How an entry leaves the phone, in the one place that decides it.
///
/// The engine holds any entry whose status it is not told as a *draft* and leaves drafts out of every statement, so the
/// status is not a detail a screen may forget: a form that sent none was "posting" entries that never reached a trial
/// balance. Both buttons on the form go through here, which is why there is only one answer to "what status does a
/// submitted entry carry".
enum JournalEntryIntent {
  /// Counts in the books from now on.
  post,

  /// Kept, listed, and left out of the statements until somebody posts it.
  keepAsDraft,
}

/// The status the engine expects for this intent.
String journalEntryStatus(JournalEntryIntent intent) =>
    intent == JournalEntryIntent.keepAsDraft ? 'draft' : 'posted';

/// Build the entry a person has finished filling in.
///
/// A draft and a posting differ only in the status they carry: the same lines, the same date, the same description, so a
/// draft that is later posted says exactly what was written.
JournalEntry buildJournalEntry({
  required String id,
  required DateTime entryDate,
  required String description,
  required List<JournalLine> lines,
  JournalEntryIntent intent = JournalEntryIntent.post,
  String? referenceNumber,
  String sourceModule = 'MobileApp',
}) {
  final reference = referenceNumber?.trim();
  return JournalEntry(
    id: id,
    entryDate: entryDate,
    description: description,
    referenceNumber: (reference == null || reference.isEmpty)
        ? null
        : reference,
    sourceModule: sourceModule,
    status: journalEntryStatus(intent),
    lines: lines,
  );
}

/// Whether this entry is held out of the statements.
///
/// An entry that arrives with no status at all counts as a draft, because that is exactly what the engine does with one:
/// the phone and the books must never disagree about whether an entry is in them.
bool isHeldAsDraft(JournalEntry entry) => (entry.status ?? 'draft') == 'draft';
