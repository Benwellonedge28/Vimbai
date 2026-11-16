// A signed-in phone has to stay signed in.
//
// The access token lasts an hour and the app used to throw the refresh token away, so the session died quietly: every
// screen said it could not load, and the app still answered "signed in". These tests hold the renewal itself - the token
// that comes back, the once-only behaviour when several calls arrive together, and a refresh token the engine refuses.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/core/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/features/authentication/models/user.dart';

void main() {
  final auth = AuthService();
  final originalTransport = ApiClient.transport;

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
  });

  tearDown(() {
    ApiClient.transport = originalTransport;
    ApiClient.beforeRequest = null;
    AuthService.onSessionEnded = null;
  });

  test('a nearly spent token is renewed and the new pair is kept', () async {
    await UserLocalData.saveAuthToken('old-access');
    await UserLocalData.saveRefreshToken('refresh-1');
    await UserLocalData.saveAuthExpiry(
      DateTime.now().subtract(const Duration(minutes: 1)),
    );

    final posted = <String>[];
    final asked = <String>[];
    ApiClient.transport = MockClient((request) async {
      asked.add(request.url.path);
      posted.add(request.body);
      return http.Response(
        jsonEncode({
          'access_token': 'new-access',
          'refresh_token': 'refresh-2',
          'expires_in': 3600,
        }),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    expect(await auth.refreshSession(), isTrue);
    // the engine's own door: /identity/token/refresh, beside /identity/users/login
    expect(asked.single, '/identity/token/refresh');
    expect(posted.single, contains('refresh_token=refresh-1'));
    expect(await UserLocalData.getAuthToken(), 'new-access');
    expect(
      await UserLocalData.getRefreshToken(),
      'refresh-2',
      reason:
          'the engine spends the old refresh token as it issues the new one',
    );
    expect(
      (await UserLocalData.getAuthExpiry())!.isAfter(DateTime.now()),
      isTrue,
    );
  });

  test('a token that is still good buys nothing', () async {
    await UserLocalData.saveAuthToken('live-access');
    await UserLocalData.saveRefreshToken('refresh-1');
    await UserLocalData.saveAuthExpiry(
      DateTime.now().add(const Duration(minutes: 30)),
    );

    var asked = 0;
    ApiClient.transport = MockClient((request) async {
      asked++;
      return http.Response('{}', 200);
    });

    expect(await auth.refreshSession(), isTrue);
    expect(asked, 0, reason: 'no reason to spend a refresh token early');
    expect(await UserLocalData.getAuthToken(), 'live-access');
  });

  test('two calls at once share one renewal', () async {
    await UserLocalData.saveAuthToken('old-access');
    await UserLocalData.saveRefreshToken('refresh-1');
    await UserLocalData.saveAuthExpiry(
      DateTime.now().subtract(const Duration(hours: 1)),
    );

    var asked = 0;
    ApiClient.transport = MockClient((request) async {
      asked++;
      await Future<void>.delayed(const Duration(milliseconds: 20));
      return http.Response(
        jsonEncode({
          'access_token': 'new-access',
          'refresh_token': 'refresh-2',
          'expires_in': 3600,
        }),
        200,
      );
    });

    final answers = await Future.wait([
      auth.refreshSession(),
      auth.refreshSession(),
      auth.refreshSession(),
    ]);
    expect(answers, everyElement(isTrue));
    expect(
      asked,
      1,
      reason: 'each refresh spends the refresh token it was given',
    );
  });

  test(
    'a refresh token the engine refuses ends the session honestly',
    () async {
      await UserLocalData.saveAuthToken('old-access');
      await UserLocalData.saveRefreshToken('spent');
      await UserLocalData.saveAuthExpiry(
        DateTime.now().subtract(const Duration(hours: 2)),
      );
      await UserLocalData.saveUser(
        User(id: 'u1', username: 'sam', email: 'sam@example.com', role: 'user'),
      );

      ApiClient.transport = MockClient((request) async {
        return http.Response('{"detail":"Invalid refresh token"}', 401);
      });

      expect(await auth.refreshSession(), isFalse);
      expect(await UserLocalData.getAuthToken(), isNull);
      expect(await UserLocalData.getRefreshToken(), isNull);
      expect(
        await auth.isLoggedIn(),
        isFalse,
        reason: 'the app stops claiming to be signed in',
      );
    },
  );

  test(
    'the app is told once when the session ends, so sign-in can come back',
    () async {
      await UserLocalData.saveAuthToken('old-access');
      await UserLocalData.saveRefreshToken('spent');
      await UserLocalData.saveAuthExpiry(
        DateTime.now().subtract(const Duration(hours: 2)),
      );

      ApiClient.transport = MockClient((request) async {
        return http.Response('{"detail":"Invalid refresh token"}', 401);
      });
      var told = 0;
      AuthService.onSessionEnded = () => told++;

      await Future.wait([auth.refreshSession(), auth.refreshSession()]);
      expect(
        told,
        1,
        reason: 'one session ended once, however many calls noticed',
      );
    },
  );

  test(
    'a request carries the renewed token, and the renewing call does not renew itself',
    () async {
      await UserLocalData.saveAuthToken('old-access');
      await UserLocalData.saveRefreshToken('refresh-1');
      await UserLocalData.saveAuthExpiry(
        DateTime.now().subtract(const Duration(minutes: 1)),
      );

      final seen = <String>[];
      ApiClient.transport = MockClient((request) async {
        seen.add('${request.method} ${request.url.path}');
        if (request.url.path.endsWith('/token/refresh')) {
          return http.Response(
            jsonEncode({
              'access_token': 'new-access',
              'refresh_token': 'refresh-2',
              'expires_in': 3600,
            }),
            200,
          );
        }
        return http.Response(
          '{}',
          200,
          headers: {'content-type': 'application/json'},
        );
      });
      ApiClient.beforeRequest = auth.refreshSession;

      final token = await UserLocalData.getAuthToken();
      await ApiClient().get(
        Uri.parse('https://example.invalid/books'),
        headers: {'Authorization': 'Bearer $token'},
      );

      expect(seen, [
        'POST /identity/token/refresh',
        'GET /books',
      ], reason: 'the renewal runs once, before the call that needed it');
      expect(await UserLocalData.getAuthToken(), 'new-access');
    },
  );
}
