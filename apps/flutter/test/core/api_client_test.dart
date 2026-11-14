// The offline rules the whole app leans on: a read is retried, a write is not, and a dead network is a typed exception
// rather than a stack trace in a page.
//
// The client's transport is replaced here, so these tests answer without a network and prove the rules instead of the
// server's mood.
import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';

void main() {
  final originalTransport = ApiClient.transport;
  final originalTimeout = ApiClient.timeout;
  tearDown(() {
    ApiClient.transport = originalTransport;
    ApiClient.timeout = originalTimeout;
  });

  test('a read is retried once and the answer returned', () async {
    var calls = 0;
    ApiClient.transport = MockClient((request) async {
      calls++;
      if (calls == 1) {
        throw const SocketException('connection refused');
      }
      return http.Response('{"ok":true}', 200);
    });

    final response = await ApiClient().get(Uri.parse('http://engine/books'));

    expect(calls, 2, reason: 'a retried read should reach the server twice');
    expect(response.statusCode, 200);
    expect(response.body, '{"ok":true}');
  });

  test('a write is not retried, so a retried POST cannot post twice', () async {
    var calls = 0;
    ApiClient.transport = MockClient((request) async {
      calls++;
      throw const SocketException('connection refused');
    });

    await expectLater(
      ApiClient().post(Uri.parse('http://engine/books'), body: '{}'),
      throwsA(isA<OfflineException>()),
    );
    expect(
      calls,
      1,
      reason: 'a write must reach the server once or not at all',
    );
  });

  test(
    'a dead network becomes an OfflineException, not a socket error',
    () async {
      ApiClient.transport = MockClient((request) async {
        throw const SocketException('no route to host');
      });

      await expectLater(
        ApiClient().get(Uri.parse('http://engine/books')),
        throwsA(isA<OfflineException>()),
      );
    },
  );

  test('a request that never answers gives up and says so', () async {
    ApiClient.timeout = const Duration(milliseconds: 50);
    ApiClient.transport = MockClient(
      (request) => Completer<http.Response>().future,
    );

    await expectLater(
      ApiClient().post(Uri.parse('http://engine/books'), body: '{}'),
      throwsA(isA<OfflineException>()),
    );
  });

  test('headers a caller sets are sent as given', () async {
    http.Request? seen;
    ApiClient.transport = MockClient((request) async {
      seen = request;
      return http.Response('[]', 200);
    });

    await ApiClient().get(
      Uri.parse('http://engine/books/a/journal-entries'),
      headers: {'x-book-id': 'book-1', 'authorization': 'Bearer token'},
    );

    expect(seen!.headers['x-book-id'], 'book-1');
    expect(seen!.headers['authorization'], 'Bearer token');
  });
}
