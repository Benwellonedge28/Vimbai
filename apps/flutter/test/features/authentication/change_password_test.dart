// Changing the password on the phone.
//
// Two things are load-bearing and easy to get wrong. The current password must actually be sent - the engine refuses the
// change without it, and a client that "helpfully" fills it in would turn a borrowed phone into an account takeover. And a
// successful change retires every refresh token the person holds, so the phone must not keep pretending it has a session it
// can renew: it ends here, deliberately, and says so.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/core/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/features/authentication/models/user.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/change_password_page.dart';

Map<String, dynamic> _storedUser() => {
  'id': 'user-1',
  'username': 'sam',
  'email': 'sam@example.com',
  'role': 'owner',
};

void main() {
  final originalTransport = ApiClient.transport;

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    await UserLocalData.saveUser(User.fromJson(_storedUser()));
    await UserLocalData.saveAuthToken('a-token');
  });

  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  test('the change names this person and sends both passwords', () async {
    Uri? seen;
    String? body;
    String? authorization;
    ApiClient.transport = MockClient((request) async {
      seen = request.url;
      body = request.body;
      authorization = request.headers['Authorization'];
      return http.Response(
        jsonEncode({'ok': true, 'message': 'Password changed successfully'}),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    final auth = AuthService();
    final changed = await auth.changePassword(
      currentPassword: 'the old password',
      newPassword: 'a long enough password',
    );

    expect(changed, isTrue);
    expect(seen!.path, endsWith('/identity/users/user-1/change-password'));
    expect(authorization, 'Bearer a-token');
    final sent = jsonDecode(body!) as Map<String, dynamic>;
    expect(sent['current_password'], 'the old password');
    expect(sent['new_password'], 'a long enough password');
  });

  test('a wrong current password is reported in the engine words', () async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode({'detail': 'Current password is incorrect'}),
        400,
        headers: {'content-type': 'application/json'},
      );
    });

    final auth = AuthService();
    final changed = await auth.changePassword(
      currentPassword: 'wrong',
      newPassword: 'a long enough password',
    );

    expect(changed, isFalse);
    expect(auth.lastResetError, 'Current password is incorrect');
    expect(
      await UserLocalData.getAuthToken(),
      'a-token',
      reason: 'a refused change must not sign the person out',
    );
  });

  test('a password rule the new one misses comes back as its reason', () async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode({'detail': 'Password must be at least 8 characters'}),
        400,
        headers: {'content-type': 'application/json'},
      );
    });

    final auth = AuthService();
    expect(
      await auth.changePassword(
        currentPassword: 'the old password',
        newPassword: 'short',
      ),
      isFalse,
    );
    expect(auth.lastResetError, contains('at least 8 characters'));
  });

  test(
    'a completed change ends the session, because the engine spent it',
    () async {
      ApiClient.transport = MockClient((request) async {
        return http.Response(
          jsonEncode({'ok': true, 'message': 'Password changed successfully'}),
          200,
          headers: {'content-type': 'application/json'},
        );
      });

      final auth = AuthService();
      expect(
        await auth.changePassword(
          currentPassword: 'the old password',
          newPassword: 'a long enough password',
        ),
        isTrue,
      );

      expect(await UserLocalData.getAuthToken(), isNull);
      expect(await UserLocalData.getUser(), isNull);
    },
  );

  testWidgets('the screen refuses mismatched new passwords without asking', (
    tester,
  ) async {
    var calls = 0;
    ApiClient.transport = MockClient((request) async {
      calls++;
      return http.Response('{"ok":true}', 200);
    });

    await tester.pumpWidget(
      MaterialApp(home: ChangePasswordPage(authService: AuthService())),
    );
    await tester.enterText(
      find.widgetWithText(TextField, 'Current password'),
      'the old password',
    );
    await tester.enterText(
      find.widgetWithText(TextField, 'New password'),
      'a long enough password',
    );
    await tester.enterText(
      find.widgetWithText(TextField, 'New password again'),
      'a different password',
    );
    await tester.tap(find.text('Change the password'));
    await tester.pumpAndSettle();

    expect(find.text('The two new passwords do not match.'), findsOneWidget);
    expect(
      calls,
      0,
      reason: 'a typo must not be sent to the engine as a request',
    );
  });
}
