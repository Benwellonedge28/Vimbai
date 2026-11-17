// What happened in a shared Book.
//
// A Book kept by several people raises a question nothing in the app answered: who moved these numbers? The engine has recorded
// it all along, against the person who did it. These tests are about the record reaching the screen - and about the one mistake
// this app has made before, saying "nothing here" when the read failed rather than when there was nothing.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/books/data/book_sync_service.dart';
import 'package:vimbai_mobile_client/features/books/presentation/book_activity_sheet.dart';

http.Response _json(Object body, [int code = 200]) => http.Response(
  jsonEncode(body),
  code,
  headers: {'content-type': 'application/json'},
);

void main() {
  final service = BookSyncService.instance;
  late Directory deviceDir;
  final originalTransport = ApiClient.transport;

  setUpAll(() {
    sqfliteFfiInit();
    databaseFactory = databaseFactoryFfi;
    deviceDir = Directory.systemTemp.createTempSync('vimbai_activity_');
    BookSyncService.databasesDirectory = deviceDir.path;
  });

  tearDownAll(() {
    BookSyncService.databasesDirectory = null;
    if (deviceDir.existsSync()) deviceDir.deleteSync(recursive: true);
  });

  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  test('the record comes back newest first, as the engine keeps it', () async {
    var asked = '';
    ApiClient.transport = MockClient((request) async {
      asked = request.url.path;
      return _json({
        'audit': [
          {
            'actor': 'user-1',
            'action': 'entries.pushed',
            'detail': '3',
            'created_at': 1763300000.5,
          },
          {
            'actor': 'user-2',
            'action': 'member.accepted',
            'detail': 'bookkeeper',
            'created_at': 1763200000.5,
          },
        ],
      });
    });
    service.setAuthToken('a-token');

    final rows = await service.activity('book-1');
    expect(asked, endsWith('/books/book-1/audit'));
    expect(rows.length, 2);
    expect(rows.first['action'], 'entries.pushed');
    expect(rows.first['detail'], '3');
  });

  testWidgets('the record is said in words, with the actor named', (
    tester,
  ) async {
    ApiClient.transport = MockClient((request) async {
      if (request.url.path.endsWith('/books/book-1/members')) {
        return _json({
          'members': [
            {
              'user_id': 'user-1',
              'role': 'owner',
              'display_name': 'Samuel',
              'status': 'active',
            },
            {
              'user_id': 'user-2',
              'role': 'bookkeeper',
              'display_name': 'Gus',
              'status': 'revoked',
            },
          ],
        });
      }
      if (request.url.path.endsWith('/books/book-1/audit')) {
        return _json({
          'audit': [
            {
              'actor': 'user-1',
              'action': 'member.updated',
              'detail': 'user-2:bookkeeper:revoked',
              'created_at': 1763300000.5,
            },
            {
              'actor': 'user-1',
              'action': 'member.invited',
              'detail': 'user-2:bookkeeper',
              'created_at': 1763200000.5,
            },
            {
              'actor': 'user-2',
              'action': 'entries.pushed',
              'detail': '2',
              'created_at': 1763100000.5,
            },
          ],
        });
      }
      return _json({}, 404);
    });
    service.setAuthToken('a-token');

    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: BookActivitySheet(
            bookId: 'book-1',
            bookName: 'Mukandara Books',
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Samuel took Gus out of this Book'), findsOneWidget);
    expect(find.text('Samuel invited somebody into this Book'), findsOneWidget);
    expect(
      find.text('Gus added entries from their device (2)'),
      findsOneWidget,
    );
  });

  testWidgets(
    'a read that failed is not reported as a Book where nothing happened',
    (tester) async {
      ApiClient.transport = MockClient(
        (request) async => _json({'error': 'unreachable'}, 503),
      );
      service.setAuthToken('a-token');

      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: BookActivitySheet(
              bookId: 'book-2',
              bookName: 'Offline Books',
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Activity is unavailable offline'), findsOneWidget);
      expect(find.text('Nothing has been recorded yet.'), findsNothing);
    },
  );
}
