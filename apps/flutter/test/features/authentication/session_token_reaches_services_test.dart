// Every service signs its requests with the session, or nothing does.
//
// Five services each kept their own copy of an access token that the app never filled, so Books, personal finance, group
// savings, household and the non-profit screens reached the engine with no token at all. A walk against the engine did
// not notice; the gateway throws away any identity a client supplies and reads only the token, so every one of those
// screens would be refused for a real person. These tests make each service show the token on the wire, and keep the
// escape hatch a test uses to give one service a token of its own.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_sync_service.dart';
import 'package:vimbai_mobile_client/features/group_savings/data/group_savings_service.dart';
import 'package:vimbai_mobile_client/features/household_finance/data/household_finance_service.dart';
import 'package:vimbai_mobile_client/features/npo/data/npo_scale_service.dart';
import 'package:vimbai_mobile_client/features/personal_finance/data/personal_finance_service.dart';

/// A transport that records what each request carried and answers with an empty list.
class _Wire {
  final List<http.BaseRequest> seen = [];

  MockClient get client => MockClient((request) async {
    seen.add(request);
    return http.Response(
      jsonEncode({
        'total': 0,
        'groups': [],
        'orgs': [],
        'debts': [],
        'recurring': [],
        'investments': [],
      }),
      200,
      headers: {'content-type': 'application/json'},
    );
  });

  String? get authorization =>
      seen.isEmpty ? null : seen.last.headers['Authorization'];
}

void main() {
  setUp(() => SessionToken.clear());
  tearDown(() => SessionToken.clear());

  test('the session token reached every service that used to send none', () async {
    SessionToken.set('the-session-token');
    final readers = <String, Future<void> Function(_Wire)>{
      'Books': (w) async {
        ApiClient.transport = w.client;
        await BookSyncService.instance.refreshBooksFromServer();
      },
      'personal finance': (w) async {
        PersonalFinanceService.instance.useClient(w.client);
        await PersonalFinanceService.instance.listRecurring();
      },
      'group savings': (w) async {
        GroupSavingsService.instance.useClient(w.client);
        await GroupSavingsService.instance.listGroups();
      },
      'household': (w) async {
        HouseholdFinanceService.instance.useClient(w.client);
        await HouseholdFinanceService.instance.analyze(
          householdId: 'house-1',
          period: '2026-11',
        );
      },
      'non-profit': (w) async {
        NpoScaleService.instance.useClient(w.client);
        await NpoScaleService.instance.myOrgs();
      },
    };

    for (final entry in readers.entries) {
      final wire = _Wire();
      try {
        await entry.value(wire);
      } catch (_) {
        // what the service does with the answer is not what this test is about; what it sent is
      }
      expect(
        wire.authorization,
        'Bearer the-session-token',
        reason:
            'the ${entry.key} service sent a request without the session token, which a real gateway refuses',
      );
    }
  });

  test(
    'a service given a token of its own still uses it, so a test can act as someone else',
    () async {
      SessionToken.set('the-session-token');
      final wire = _Wire();
      ApiClient.transport = wire.client;
      BookSyncService.instance.setAuthToken('another-token');
      try {
        await BookSyncService.instance.refreshBooksFromServer();
      } catch (_) {}
      expect(wire.authorization, 'Bearer another-token');
    },
  );

  test('signing out leaves no token for the next request to carry', () {
    SessionToken.set('the-session-token');
    SessionToken.clear();
    expect(SessionToken.value, isNull);
    expect(SessionToken.or(null), isNull);
  });
}
