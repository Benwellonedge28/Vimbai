// Opening the first Book while the engine cannot be reached has to fail as the app's own offline failure.
//
// Book calls used to run on a second, separately-configured connection, so an unreachable engine came back as a raw socket
// error and every page that only knows "offline or not" had to guess. They run through the app's shared client now, which
// maps a dropped connection to OfflineException with the standard timeout.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/books/data/book_sync_service.dart';

void main() {
  // the Book registry is kept on the device, so a test needs the same store the phone has
  setUpAll(() {
    sqfliteFfiInit();
    databaseFactory = databaseFactoryFfi;
  });

  final originalTransport = ApiClient.transport;

  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  test(
    'a Book opened with no engine reads as offline, not a socket error',
    () async {
      ApiClient.transport = MockClient((_) async {
        throw const SocketException('engine unreachable');
      });
      BookSyncService.instance.setAuthToken('a-token');

      await expectLater(
        BookSyncService.instance.createBook('Mukandara Books', 'personal'),
        throwsA(isA<OfflineException>()),
      );
    },
  );

  test('a Book the engine opens still reads back from the answer', () async {
    late Uri asked;
    ApiClient.transport = MockClient((request) async {
      asked = request.url;
      // the device is registered right after the Book opens, so the engine is asked about that Book by name
      if (request.url.path.endsWith('/devices')) {
        return http.Response(
          jsonEncode({'device_id': 'device-1'}),
          200,
          headers: {'content-type': 'application/json'},
        );
      }
      return http.Response(
        jsonEncode({
          'book': {
            'id': 'book-1',
            'name': 'Mukandara Books',
            'tier': 'personal',
            'description': '',
            'folder': '',
            'role': 'owner',
            'status': 'active',
          },
        }),
        200,
        headers: {'content-type': 'application/json'},
      );
    });
    BookSyncService.instance.setAuthToken('a-token');

    final book = await BookSyncService.instance.createBook(
      'Mukandara Books',
      'personal',
    );

    expect(book.id, 'book-1');
    expect(book.name, 'Mukandara Books');
    // the Book is opened on the book-sync door, and the device is registered under it afterwards
    expect(asked.path, endsWith('/book-sync/books/book-1/devices'));
    // the key is made on the device and sent wrapped, so the engine never holds it in the clear
    expect(asked.path, isNot(contains('key=')));
  });
}
