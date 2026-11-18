// Signing out ends the session at the engine too.
//
// Logout used to forget the tokens on the phone and nothing else. A refresh token is good for days, so a phone handed to
// someone else - which is exactly the stranger's first run - left a session that could still be renewed by anything that
// had copied the token. The engine has a door that ends one; sign-out now walks through it, and still signs out when the
// engine cannot be reached, because being offline must never keep a person signed in on a phone they are walking away
// from.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/core/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';

void main() {
  late http.Client originalTransport;

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    originalTransport = ApiClient.transport;
    SessionToken.clear();
  });

  tearDown(() {
    ApiClient.transport = originalTransport;
    SessionToken.clear();
  });

  test(
    'signing out tells the engine to end the session, with this phone\'s refresh token',
    () async {
      await UserLocalData.saveAuthToken('an-access-token');
      await UserLocalData.saveRefreshToken('a-refresh-token');
      SessionToken.set('an-access-token');

      final asked = <Uri>[];
      ApiClient.transport = MockClient((request) async {
        asked.add(request.url);
        return http.Response(jsonEncode({'ok': true}), 200);
      });

      await AuthService().logout();

      expect(
        asked,
        isNotEmpty,
        reason: 'nothing was told, so the session outlived the sign-out',
      );
      expect(asked.last.path, endsWith('/identity/token/revoke'));
      expect(
        asked.last.queryParameters['refresh_token'],
        'a-refresh-token',
        reason: 'the token to end is this phone\'s own, not somebody else\'s',
      );
      expect(SessionToken.value, isNull);
      expect(await UserLocalData.getAuthToken(), isNull);
      expect(await UserLocalData.getRefreshToken(), isNull);
    },
  );

  test('a sign-out with no engine still signs out', () async {
    await UserLocalData.saveAuthToken('an-access-token');
    await UserLocalData.saveRefreshToken('a-refresh-token');
    SessionToken.set('an-access-token');

    ApiClient.transport = MockClient(
      (_) async => throw const SocketExceptionLike(),
    );

    await AuthService().logout();

    expect(SessionToken.value, isNull);
    expect(await UserLocalData.getAuthToken(), isNull);
  });
}

/// Stands in for a dead connection without importing dart:io into a test that may run on the web target.
class SocketExceptionLike implements Exception {
  const SocketExceptionLike();
  @override
  String toString() => 'connection refused';
}
