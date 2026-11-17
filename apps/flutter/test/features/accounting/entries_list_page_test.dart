import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entries_list_page.dart';

/// An engine that answers without a phone: the page's own work is what is under test here, and the call it makes has
/// its own coverage against the transport.
class _ScriptedService extends AccountingApiService {
  _ScriptedService({required this.entries, this.refusal});

  final List<JournalEntry> Function() entries;

  /// What the engine says when it will not post the note.
  final String? refusal;

  final posted = <String>[];

  @override
  Future<List<JournalEntry>> getJournalEntries() async => entries();

  @override
  Future<JournalEntry> postJournalEntry(String entryId) async {
    posted.add(entryId);
    if (refusal != null) throw Exception(refusal);
    return _entry('e-1', 'Consulting income', 'posted');
  }
}

/// An engine that really throws a note away, so the list after the tap is the one the engine would answer with.
class _DiscardingService extends AccountingApiService {
  _DiscardingService({required this.rows, this.refusal});

  final List<JournalEntry> rows;
  final String? refusal;
  final discarded = <String>[];

  @override
  Future<List<JournalEntry>> getJournalEntries() async => List.of(rows);

  @override
  Future<void> deleteJournalEntry(String entryId) async {
    if (refusal != null) throw Exception(refusal);
    discarded.add(entryId);
    rows.removeWhere((row) => row.id == entryId);
  }
}

JournalEntry _entry(String id, String description, String? status) =>
    JournalEntry(
      id: id,
      entryDate: DateTime(2026, 11, 15),
      description: description,
      sourceModule: 'MobileApp',
      status: status,
      lines: const [],
    );

void main() {
  testWidgets('a draft says so and can be posted from the list', (
    tester,
  ) async {
    // The statements leave drafts out, so a row that looked like any other posting would tell the person their books
    // had moved when they had not. The row says Draft, offers the one thing a draft is for, and after the posting the
    // row is an ordinary entry - the note became the entry.
    var posted = false;
    final service = _ScriptedService(
      entries: () => [
        _entry('e-1', 'Consulting income', posted ? 'posted' : 'draft'),
        _entry('e-2', 'Rent paid', 'posted'),
      ],
    );

    await tester.pumpWidget(
      MaterialApp(home: JournalEntriesListPage(service: service)),
    );
    await tester.pumpAndSettle();

    expect(find.text('Draft'), findsOneWidget);
    expect(find.widgetWithText(TextButton, 'Post'), findsOneWidget);
    expect(find.textContaining('not in the statements yet'), findsOneWidget);
    // the finished entry beside it is not offered a posting
    expect(find.text('Rent paid'), findsOneWidget);

    posted = true;
    await tester.tap(find.widgetWithText(TextButton, 'Post'));
    await tester.pumpAndSettle();

    expect(service.posted, ['e-1'], reason: 'the tap has to reach the engine');
    expect(find.text('Draft'), findsNothing);
    expect(find.widgetWithText(TextButton, 'Post'), findsNothing);
    expect(find.text('Posted "Consulting income"'), findsOneWidget);
  });

  testWidgets('a refusal is shown with the engine wording', (tester) async {
    final service = _ScriptedService(
      entries: () => [_entry('e-1', 'Consulting income', 'draft')],
      refusal: 'That entry is already posted',
    );

    await tester.pumpWidget(
      MaterialApp(home: JournalEntriesListPage(service: service)),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(TextButton, 'Post'));
    await tester.pumpAndSettle();

    expect(find.textContaining('already posted'), findsWidgets);
    // the row is still a draft: a refusal must not look like the books moved
    expect(find.text('Draft'), findsOneWidget);
  });

  testWidgets('a note can be thrown away after asking, and the row goes', (
    tester,
  ) async {
    // Save as draft was a one-way door: a note saved by mistake could only be posted by mistake. Discarding it moves
    // nothing in the statements, so the row leaves the list rather than sitting there waiting to be posted.
    final service = _DiscardingService(
      rows: [
        _entry('e-1', 'Consulting income', 'draft'),
        _entry('e-2', 'Rent paid', 'posted'),
      ],
    );

    await tester.pumpWidget(
      MaterialApp(home: JournalEntriesListPage(service: service)),
    );
    await tester.pumpAndSettle();

    // nothing leaves the list until the person agrees
    await tester.tap(find.widgetWithText(TextButton, 'Discard'));
    await tester.pumpAndSettle();
    expect(service.discarded, isEmpty);
    expect(find.textContaining('never posted'), findsOneWidget);

    await tester.tap(find.text('Throw it away'));
    await tester.pumpAndSettle();

    expect(service.discarded, [
      'e-1',
    ], reason: 'the tap has to reach the engine');
    expect(find.text('Consulting income'), findsNothing);
    expect(find.text('Rent paid'), findsOneWidget);
    // the posted entry beside it is never offered a discard: it is corrected instead
    expect(find.widgetWithText(TextButton, 'Discard'), findsNothing);
  });

  testWidgets('a refused discard keeps the note and says why', (tester) async {
    // A posted entry is corrected, never deleted. If the engine refuses, the row must stay: a row that vanished while
    // the posting still stood would tell the person their books had changed when they had not.
    final service = _DiscardingService(
      rows: [_entry('e-3', 'Rent paid', 'draft')],
      refusal:
          'A posted journal entry cannot be deleted; correct it with a reversing entry.',
    );

    await tester.pumpWidget(
      MaterialApp(home: JournalEntriesListPage(service: service)),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(TextButton, 'Discard'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Throw it away'));
    await tester.pumpAndSettle();

    expect(find.textContaining('Could not throw it away'), findsOneWidget);
    expect(
      find.textContaining('correct it with a reversing entry'),
      findsOneWidget,
    );
    expect(find.text('Rent paid'), findsOneWidget);
  });
}
