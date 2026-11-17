import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entry_detail_page.dart';

JournalEntry _entry(String status) => JournalEntry.fromJson({
  'id': 'e-1',
  'entry_date': '2026-11-15',
  'description': 'first sale',
  'reference_number': 'INV-1',
  'source_module': 'Manual',
  'status': status,
  'lines': [
    {'account_number': '1000', 'debit': '250.00', 'credit': '0'},
    {'account_number': '4000', 'debit': '0', 'credit': '250.00'},
  ],
});

/// The engine's answer, standing in for the network: the page's job is to ask once, ask before writing, and report what
/// came back, and the answers below are the ones the engine gives.
class _AnsweringService extends AccountingApiService {
  _AnsweringService({this.refusal});

  final String? refusal;
  final List<String> asked = [];

  @override
  Future<JournalEntry> reverseJournalEntry(
    String entryId, {
    String? entryDate,
  }) async {
    asked.add(entryId);
    if (refusal != null) throw Exception(refusal);
    return JournalEntry.fromJson({
      'id': 'e-2',
      'entry_date': '2026-11-15',
      'description': 'Reversal of first sale',
      'reference_number': 'INV-1-R',
      'source_module': 'Manual',
      'status': 'posted',
      'reverses_entry_id': 'e-1',
      'lines': [
        {'account_number': '1000', 'debit': '0', 'credit': '250.00'},
        {'account_number': '4000', 'debit': '250.00', 'credit': '0'},
      ],
    });
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late http.Client originalTransport;

  setUpAll(() {
    sqfliteFfiInit();
    databaseFactory = databaseFactoryFfi;
  });
  setUp(() {
    SharedPreferences.setMockInitialValues({});
    originalTransport = ApiClient.transport;
  });
  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  testWidgets('a posted entry offers a correction, and writing it says what happened', (
    tester,
  ) async {
    // The correction is the engine's, so the app asks once at the entry's own door and reports what came back. The
    // amounts are never flipped on the phone: a second copy of that rule could differ from the one the books obey.
    final service = _AnsweringService();

    await tester.pumpWidget(
      MaterialApp(
        home: JournalEntryDetailPage(entry: _entry('posted'), service: service),
      ),
    );
    await tester.pumpAndSettle();

    // the door sits below the entry's lines, so the page is scrolled to it the way a person would be
    await tester.ensureVisible(find.text('Correct this entry'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Correct this entry'));
    await tester.pumpAndSettle();
    // nothing is written until the person agrees, because a correction moves the books
    expect(service.asked, isEmpty);

    await tester.tap(find.text('Write the correction'));
    await tester.pumpAndSettle();

    expect(service.asked, ['e-1']);
    expect(find.textContaining('The correction was written'), findsOneWidget);
    // the door closes behind it: the engine refuses a second correction, so it is not offered again
    expect(find.text('Correct this entry'), findsNothing);
    expect(find.textContaining('has been corrected'), findsOneWidget);
  });

  testWidgets('a refused correction keeps the door open and says why', (tester) async {
    // A refusal is the engine's own words coming back: the entry already had a correction, say. Swallowing it would
    // leave the person believing the books were fixed.
    final service = _AnsweringService(
      refusal: 'That entry has already been reversed by e-9.',
    );
    await tester.pumpWidget(
      MaterialApp(
        home: JournalEntryDetailPage(entry: _entry('posted'), service: service),
      ),
    );
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('Correct this entry'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Correct this entry'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Write the correction'));
    await tester.pumpAndSettle();

    expect(find.textContaining('Could not correct it'), findsOneWidget);
    expect(find.textContaining('already been reversed'), findsOneWidget);
    expect(find.text('Correct this entry'), findsOneWidget);
  });

  testWidgets('a correction left undone changes nothing', (tester) async {
    final service = _AnsweringService();

    await tester.pumpWidget(
      MaterialApp(
        home: JournalEntryDetailPage(entry: _entry('posted'), service: service),
      ),
    );
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('Correct this entry'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Correct this entry'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Keep it as it is'));
    await tester.pumpAndSettle();

    expect(service.asked, isEmpty);
    expect(find.text('Correct this entry'), findsOneWidget);
  });

  testWidgets('a draft is not offered a correction', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: JournalEntryDetailPage(
          entry: _entry('draft'),
          service: _AnsweringService(),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Correct this entry'), findsNothing);
  });

  test('the correction is asked for at the entry\'s own door', () async {
    final calls = <String>[];
    String? body;
    ApiClient.transport = MockClient((request) async {
      calls.add('${request.method} ${request.url.path}');
      body = request.body;
      return http.Response(
        jsonEncode({
          'id': 'e-2',
          'entry_date': '2026-11-15',
          'description': 'Reversal of first sale',
          'reference_number': 'INV-1-R',
          'source_module': 'Manual',
          'status': 'posted',
          'lines': [
            {'account_number': '1000', 'debit': '0', 'credit': '250.00'},
            {'account_number': '4000', 'debit': '250.00', 'credit': '0'},
          ],
        }),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    final correction = await AccountingApiService().reverseJournalEntry('e-1');

    expect(calls, ['POST /accounting/journal-entries/e-1/reverse']);
    // no date is invented on the phone: left alone, the engine dates the correction with the original
    expect(body, isNot(contains('entry_date')));
    expect(correction.id, 'e-2');
    expect(correction.status, 'posted');
  });

  test('a refused correction says why', () async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode({'error': 'That entry has already been reversed by e-9.'}),
        422,
        headers: {'content-type': 'application/json'},
      );
    });
    await expectLater(
      AccountingApiService().reverseJournalEntry('e-1'),
      throwsA(
        predicate(
          (error) => error.toString().contains('already been reversed'),
          'carries the reason',
        ),
      ),
    );
  });
}
