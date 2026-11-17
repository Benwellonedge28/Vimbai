import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() {
    // the service keeps a local copy of what it reads, so the test needs a database engine rather than a phone
    sqfliteFfiInit();
    databaseFactory = databaseFactoryFfi;
  });
  setUp(() {
    // the service reads the session token through the auth service before it calls anything
    SharedPreferences.setMockInitialValues({});
  });
  // the app's shared client builds a real transport, which only exists inside a test zone
  late http.Client originalTransport;

  setUp(() {
    originalTransport = ApiClient.transport;
  });

  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  test('posting a draft asks the engine once, at the note\'s own address', () async {
    // Posting a note is one door on the engine and it is the moment the books move, so the app must not invent a second
    // posting of its own, must not queue it as though the work were done, and must not swallow a refusal.
    final calls = <String>[];
    String? sentTo;
    ApiClient.transport = MockClient((request) async {
      calls.add('${request.method} ${request.url.path}');
      sentTo = request.url.path;
      return http.Response(
        jsonEncode({
          'id': 'e-7',
          'entry_date': '2026-11-15',
          'description': 'Consulting income',
          'source_module': 'MobileApp',
          'status': 'posted',
          'lines': [
            {'account_number': '1000', 'debit': '25.00', 'credit': '0'},
            {'account_number': '4000', 'debit': '0', 'credit': '25.00'},
          ],
        }),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    final posted = await AccountingApiService().postJournalEntry('e-7');

    expect(sentTo, endsWith('/journal-entries/e-7/post'));
    // one call, a POST, and nothing else: no second entry written beside the posting
    expect(calls.length, 1, reason: '$calls');
    expect(calls.single.startsWith('POST'), isTrue, reason: '$calls');
    expect(posted.id, 'e-7');
    expect(posted.status, 'posted');
    expect(posted.lines.length, 2);
  });

  test(
    'a refused posting is an error with the engine\'s reason, not a silent success',
    () async {
      ApiClient.transport = MockClient((request) async {
        return http.Response(
          jsonEncode({'error': 'That entry is already posted'}),
          422,
          headers: {'content-type': 'application/json'},
        );
      });

      await expectLater(
        AccountingApiService().postJournalEntry('e-7'),
        throwsA(
          predicate(
            (error) => error.toString().contains('already posted'),
            'carries the reason',
          ),
        ),
      );
    },
  );
}
