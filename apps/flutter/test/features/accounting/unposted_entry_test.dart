import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entry_form_page.dart';

/// An engine that cannot be reached, and one that answers.
class _Unreachable extends AccountingApiService {
  final posted = <JournalEntry>[];

  @override
  Future<List<Account>> getAllAccounts() async => const [];

  @override
  Future<JournalEntry> createJournalEntry(JournalEntry entry) async {
    throw OfflineException('no engine at /journal-entries');
  }
}

class _Answering extends AccountingApiService {
  final posted = <JournalEntry>[];

  @override
  Future<List<Account>> getAllAccounts() async => const [];

  @override
  Future<JournalEntry> createJournalEntry(JournalEntry entry) async {
    posted.add(entry);
    return entry;
  }
}

/// The form is long and its buttons sit at the bottom, so a test gets a phone-shaped page tall enough to hold it whole
/// rather than tapping a spot that is off the screen.
void _tallPhone(WidgetTester tester) {
  tester.view.physicalSize = const Size(900, 2200);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
}

Future<void> _fillBalanced(WidgetTester tester) async {
  await tester.tap(find.widgetWithText(ElevatedButton, 'Add Journal Line'));
  await tester.pumpAndSettle();
  await tester.enterText(
    find.widgetWithText(TextFormField, 'Description'),
    'Sold goods for cash',
  );
  await tester.tap(find.widgetWithText(ElevatedButton, 'Add Journal Line'));
  await tester.pumpAndSettle();
  await tester.pumpAndSettle();

  final accounts = find.widgetWithText(TextFormField, 'Account Number');
  await tester.enterText(accounts.at(0), '1000');
  await tester.enterText(accounts.at(1), '4000');
  await tester.enterText(
    find.widgetWithText(TextFormField, 'Debit').at(0),
    '25',
  );
  await tester.enterText(
    find.widgetWithText(TextFormField, 'Credit').at(1),
    '25',
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('an entry the engine never took is not kept as one', (
    tester,
  ) async {
    // Writing an entry into the phone's own journal and calling it saved put a posting into the statements that no ledger
    // held: an offline entry has no hash chain, no balance and no Book. The person is told it was not posted instead, and
    // their lines stay on the form.
    final service = _Unreachable();
    _tallPhone(tester);
    await tester.pumpWidget(
      MaterialApp(home: JournalEntryFormPage(service: service)),
    );
    await tester.pumpAndSettle();
    await _fillBalanced(tester);

    await tester.tap(
      find.widgetWithText(ElevatedButton, 'Submit Journal Entry'),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('was not posted'), findsOneWidget);
    expect(
      find.textContaining('Nothing has been recorded'),
      findsOneWidget,
      reason: 'the truth, in words the person can act on',
    );
    expect(
      find.byType(JournalEntryFormPage),
      findsOneWidget,
      reason: 'the form stays, so the work is not lost',
    );
    expect(
      find.text('Total Debits: 25.00'),
      findsOneWidget,
      reason: 'the lines are untouched, so the person can submit again',
    );
  });

  testWidgets('a reachable engine posts the entry and leaves the form', (
    tester,
  ) async {
    final service = _Answering();
    _tallPhone(tester);
    await tester.pumpWidget(
      MaterialApp(home: JournalEntryFormPage(service: service)),
    );
    await tester.pumpAndSettle();
    await _fillBalanced(tester);

    await tester.tap(
      find.widgetWithText(ElevatedButton, 'Submit Journal Entry'),
    );
    await tester.pumpAndSettle();

    expect(service.posted, hasLength(1));
    expect(service.posted.single.lines, hasLength(2));
    expect(service.posted.single.lines.first.accountNumber, '1000');
    expect(service.posted.single.lines.last.accountNumber, '4000');
  });

  testWidgets('a draft can be kept whatever the network says', (tester) async {
    _tallPhone(tester);
    await tester.pumpWidget(
      MaterialApp(home: JournalEntryFormPage(service: _Answering())),
    );
    await tester.pumpAndSettle();
    await _fillBalanced(tester);

    // the draft button used to disappear when a connectivity check said offline, hiding the honest path exactly then
    expect(find.widgetWithText(TextButton, 'Save as draft'), findsOneWidget);
    expect(find.textContaining('Connectivity:'), findsNothing);
  });
}
