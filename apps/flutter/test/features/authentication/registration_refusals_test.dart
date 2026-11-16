// Registering an account, and what the person is told when it does not happen.
//
// The page said "Registration failed. Please try again." for every cause, including the one a stranger actually hits:
// the username is taken. And a successful registration used to leave a stored profile with no session behind it - a name
// on the phone that no token backed, which an older build treated as enough to get in.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/core/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/register_page.dart';

void main() {
  final auth = AuthService();
  final originalTransport = ApiClient.transport;

  setUp(() => SharedPreferences.setMockInitialValues({}));

  tearDown(() => ApiClient.transport = originalTransport);

  test('a taken username is named as taken, in the engine\'s words', () async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        '{"detail":"Username already taken","error":"Username already taken"}',
        400,
      );
    });
    expect(
      await auth.register('sam', 'sam@example.com', 'a good password', 'user'),
      isFalse,
    );
    expect(auth.lastError, 'Username already taken');
  });

  test('a refused password keeps the engine\'s own reason', () async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        '{"error":"Choose a password that is not commonly used"}',
        400,
      );
    });
    expect(
      await auth.register('sam', 'sam@example.com', 'password123', 'user'),
      isFalse,
    );
    expect(auth.lastError, 'Choose a password that is not commonly used');
  });

  test('an unreachable engine says no account was created', () async {
    ApiClient.transport = MockClient((request) async {
      throw http.ClientException('Connection refused');
    });
    expect(
      await auth.register('sam', 'sam@example.com', 'a good password', 'user'),
      isFalse,
    );
    expect(auth.lastError, contains('no account was created'));
  });

  test('a created account leaves no half-session on the phone', () async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode({
          'id': 'u-new',
          'email': 'sam@example.com',
          'username': 'sam',
        }),
        201,
      );
    });
    expect(
      await auth.register('sam', 'sam@example.com', 'a good password', 'user'),
      isTrue,
    );
    expect(auth.lastError, isNull);
    expect(
      await UserLocalData.getUser(),
      isNull,
      reason: 'a profile with no token is not a session',
    );
    expect(await UserLocalData.getAuthToken(), isNull);
    expect(await auth.isLoggedIn(), isFalse);
  });

  testWidgets('the register screen shows why the account was not created', (
    tester,
  ) async {
    ApiClient.transport = MockClient((request) async {
      return http.Response('{"error":"Username already taken"}', 400);
    });

    await tester.pumpWidget(const MaterialApp(home: RegisterPage()));
    await tester.enterText(find.byType(TextField).at(0), 'sam');
    await tester.enterText(find.byType(TextField).at(1), 'sam@example.com');
    await tester.enterText(find.byType(TextField).at(2), 'a good password');
    await tester.tap(find.widgetWithText(ElevatedButton, 'Register'));
    await tester.pumpAndSettle();

    expect(find.text('Username already taken'), findsOneWidget);
    expect(find.textContaining('Please try again'), findsNothing);
  });
}
