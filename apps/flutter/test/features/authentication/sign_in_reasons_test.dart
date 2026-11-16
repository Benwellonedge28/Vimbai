// A refused sign-in has to say why.
//
// The page used to give one sentence for every cause, so a person with the right password kept retyping it while the real
// fact - an account asking for a second factor, an engine that was not running, too many wrong guesses - went to a print
// nobody on a phone can see.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/login_page.dart';

void main() {
  final auth = AuthService();
  final originalTransport = ApiClient.transport;

  setUp(() => SharedPreferences.setMockInitialValues({}));

  tearDown(() => ApiClient.transport = originalTransport);

  MockClient answering(http.Response response) =>
      MockClient((request) async => response);

  test('a wrong password is named as a wrong password', () async {
    ApiClient.transport = answering(
      http.Response('{"detail":"Incorrect username or password"}', 401),
    );
    expect(await auth.login('sam', 'nope'), isFalse);
    expect(auth.lastError, 'That username and password were not accepted.');
  });

  test(
    'an account wanting a second factor is not called a wrong password',
    () async {
      ApiClient.transport = answering(
        http.Response(
          jsonEncode({
            'mfa_required': true,
            'method': 'totp',
            'temp_token': 't',
          }),
          200,
          headers: {'content-type': 'application/json'},
        ),
      );
      expect(await auth.login('sam', 'right'), isFalse);
      expect(auth.lastError, contains('second factor'));
      expect(auth.lastError, isNot(contains('password were not')));
    },
  );

  test('the throttle states the engine\'s own wait in minutes', () async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        '{"error":"Too many sign-in attempts.","retry_after":900}',
        429,
        headers: {'retry-after': '900'},
      );
    });
    expect(await auth.login('sam', 'wrong'), isFalse);
    expect(
      auth.lastError,
      'Too many wrong attempts. Try again in about 15 minutes.',
    );
  });

  test('a short pause is given in seconds', () async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        '{"retry_after":30}',
        429,
        headers: {'retry-after': '30'},
      );
    });
    expect(await auth.login('sam', 'wrong'), isFalse);
    expect(auth.lastError, 'Too many wrong attempts. Try again in 30 seconds.');
  });

  test('an inactive account is told to ask an administrator', () async {
    ApiClient.transport = answering(
      http.Response('{"detail":"Account is not active"}', 403),
    );
    expect(await auth.login('sam', 'right'), isFalse);
    expect(auth.lastError, contains('not active'));
  });

  test('an engine that cannot be reached is said to be unreachable', () async {
    ApiClient.transport = MockClient((request) async {
      throw http.ClientException('Connection refused');
    });
    expect(await auth.login('sam', 'right'), isFalse);
    expect(auth.lastError, contains('could not be reached'));
  });

  pageTests();

  test('a good sign-in clears the last reason', () async {
    ApiClient.transport = answering(http.Response('{}', 401));
    expect(await auth.login('sam', 'wrong'), isFalse);
    expect(auth.lastError, isNotNull);

    ApiClient.transport = MockClient((request) async {
      if (request.url.path.endsWith('/users/login')) {
        return http.Response(
          jsonEncode({
            'access_token': 'a',
            'refresh_token': 'r',
            'expires_in': 3600,
          }),
          200,
        );
      }
      return http.Response(
        jsonEncode({'id': 'u1', 'username': 'sam', 'email': 'sam@example.com'}),
        200,
      );
    });
    expect(await auth.login('sam', 'right'), isTrue);
    expect(auth.lastError, isNull);
  });
}

// The page, not just the service: the reason has to reach the screen. It builds its own AuthService, which shares the
// app's one transport, so the page can be driven with an engine that answers without a network.
void pageTests() {
  testWidgets('the sign-in screen shows the reason the engine gave', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode({'mfa_required': true, 'method': 'totp', 'temp_token': 't'}),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    await tester.pumpWidget(const MaterialApp(home: LoginPage()));
    await tester.enterText(find.byType(TextField).at(0), 'sam');
    await tester.enterText(find.byType(TextField).at(1), 'right');
    await tester.tap(find.widgetWithText(ElevatedButton, 'Login'));
    await tester.pumpAndSettle();

    expect(find.textContaining('second factor'), findsOneWidget);
    expect(
      find.textContaining('Check your username and password'),
      findsNothing,
    );
    ApiClient.transport = http.Client();
  });
}
