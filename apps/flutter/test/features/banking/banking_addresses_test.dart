// The addresses the banking screens ask, proven against a transport rather than a server.
//
// These six calls had no working address at all: they asked the retired banking-integration service's paths
// (`/connect`, `/connections`, `/sync/{id}`, `/transactions/{id}`, `/transactions/{id}/reconcile`), which nothing answers,
// so a bank account could not be connected or listed on any machine. Each test names the address it expects, so a future
// edit that drifts back to a path the engine does not answer fails here rather than as an empty screen.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/banking/data/banking_api_service.dart';

void main() {
  final originalTransport = ApiClient.transport;

  setUp(() {
    // the service reads the token from local storage, so the store is stubbed rather than absent
    TestWidgetsFlutterBinding.ensureInitialized();
    SharedPreferences.setMockInitialValues({});
  });

  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  test('connecting a bank posts the connection to the feed', () async {
    late http.Request asked;
    ApiClient.transport = MockClient((request) async {
      asked = request;
      return http.Response(
        jsonEncode({
          'id': 'conn-1',
          'bank_name': 'CBZ',
          'account_number': '0123',
          'account_type': 'checking',
          'status': 'active',
          'created_at': '2026-11-15T09:00:00Z',
        }),
        201,
      );
    });

    final connection = await BankingApiService().connectBank(
      bankName: 'CBZ',
      accountNumber: '0123',
      apiKey: 'k',
    );

    expect(connection.id, 'conn-1');
    expect(asked.method, 'POST');
    expect(asked.url.path, endsWith('/bank-feed/connections'));
    expect(asked.url.queryParameters['bank_name'], 'CBZ');
  });

  test('listing connections reads the feed', () async {
    late Uri asked;
    ApiClient.transport = MockClient((request) async {
      asked = request.url;
      return http.Response('[]', 200);
    });

    final connections = await BankingApiService().getConnections();

    expect(connections, isEmpty);
    expect(asked.path, endsWith('/bank-feed/connections'));
  });

  test('syncing names the connection to the sync door', () async {
    late http.Request asked;
    ApiClient.transport = MockClient((request) async {
      asked = request;
      return http.Response('{"sync_id":"s1","status":"started"}', 200);
    });

    await BankingApiService().syncTransactions('conn-1');

    expect(asked.url.path, endsWith('/bank-feed/sync/start'));
    expect(jsonDecode(asked.body)['bank_connection_id'], 'conn-1');
  });

  test('the feed is listed for one connection, paged', () async {
    late Uri asked;
    ApiClient.transport = MockClient((request) async {
      asked = request.url;
      return http.Response('[]', 200);
    });

    await BankingApiService().getTransactions('conn-1', limit: 10, offset: 20);

    expect(asked.path, endsWith('/bank-feed/transactions'));
    expect(asked.queryParameters['connection_id'], 'conn-1');
    expect(asked.queryParameters['limit'], '10');
    expect(asked.queryParameters['offset'], '20');
  });

  test('reconciling names the transaction, not the connection', () async {
    late http.Request asked;
    ApiClient.transport = MockClient((request) async {
      asked = request;
      return http.Response('{}', 200);
    });

    await BankingApiService().reconcileTransaction(
      'conn-1',
      'txn-9',
      matchedEntryId: 'entry-3',
    );

    expect(asked.url.path, endsWith('/bank-feed/transactions/txn-9/reconcile'));
    expect(jsonDecode(asked.body)['matched_entry_id'], 'entry-3');
  });

  test('disconnecting deletes the connection', () async {
    late http.Request asked;
    ApiClient.transport = MockClient((request) async {
      asked = request;
      return http.Response('', 204);
    });

    await BankingApiService().disconnect('conn-1');

    expect(asked.method, 'DELETE');
    expect(asked.url.path, endsWith('/bank-feed/connections/conn-1'));
  });

  test('a feed row is read from the names the engine writes', () async {
    // the feed writes `bank_connection_id` and `date`; the screen used to require `connection_id` and
    // `transaction_date`, so the first row it received threw instead of showing
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode([
          {
            'id': 'txn-1',
            'bank_connection_id': 'conn-1',
            'date': '2026-11-15',
            'amount': '250.000000',
            'transaction_type': 'credit',
            'description': 'a sale',
            'status': 'cleared',
          },
        ]),
        200,
      );
    });

    final rows = await BankingApiService().getTransactions('conn-1');

    expect(rows, hasLength(1));
    expect(rows.single.connectionId, 'conn-1');
    expect(rows.single.transactionDate.year, 2026);
    expect(rows.single.transactionType, 'credit');
    expect(rows.single.reconciled, isFalse);
  });

  test('a reconciled row says so, whether by its time or its status', () async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode([
          {
            'id': 'txn-2',
            'bank_connection_id': 'conn-1',
            'date': '2026-11-15',
            'amount': '10.000000',
            'transaction_type': 'debit',
            'reconciled_at': '2026-11-15T10:00:00Z',
          },
          {
            'id': 'txn-3',
            'bank_connection_id': 'conn-1',
            'date': '2026-11-15',
            'amount': '20.000000',
            'transaction_type': 'credit',
            'status': 'reconciled',
          },
        ]),
        200,
      );
    });

    final rows = await BankingApiService().getTransactions('conn-1');

    expect(rows.map((row) => row.reconciled), everyElement(isTrue));
  });
}
