// Getting back in after a forgotten password.
//
// This is the one door a person reaches for when the app has already refused them, so it must not be a prettier version of
// the same dead end. Two things are load-bearing: the request step never says whether an address has an account (the engine
// answers conditionally on purpose), and a code that the engine refuses is shown with the engine's own reason rather than
// being retried or swallowed.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/login_page.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/password_reset_page.dart';

void main() {
  final originalTransport = ApiClient.transport;

  setUp(() {
    SharedPreferences.setMockInitialValues({});
  });

  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  test('a request for a code shows the engine words, not a claim', () async {
    String? body;
    ApiClient.transport = MockClient((request) async {
      if (request.url.path.endsWith('/identity/password/reset')) {
        body = request.body;
        return http.Response(
          jsonEncode({
            'message':
                'If that address belongs to an account, a reset code is now valid for 24 hours and is recorded in this '
                "server's audit trail, because this Vimbai server has no email delivery yet. Ask whoever runs it for the "
                'code.',
          }),
          200,
          headers: {'content-type': 'application/json'},
        );
      }
      return http.Response('{}', 200);
    });

    final auth = AuthService();
    final asked = await auth.requestPasswordReset('sam@example.com');

    expect(asked, isTrue);
    expect(
      jsonDecode(body!)['email'],
      'sam@example.com',
      reason: 'the address goes to the engine untouched apart from trimming',
    );
    expect(
      auth.lastResetMessage,
      contains('If that address belongs to an account'),
      reason:
          'the conditional wording is the point: it must not become "we sent it", and the sentence saying where the code '
          'actually is must reach the person unchanged',
    );
  });

  test('an unreachable engine says so instead of claiming a link', () async {
    ApiClient.transport = MockClient((request) async {
      throw http.ClientException('connection refused');
    });

    final auth = AuthService();
    final asked = await auth.requestPasswordReset('sam@example.com');

    expect(asked, isFalse);
    expect(auth.lastResetMessage, isNull);
    expect(auth.lastResetError, contains('could not be reached'));
  });

  test('a refused code is reported with the engine own reason', () async {
    String? body;
    ApiClient.transport = MockClient((request) async {
      if (request.url.path.endsWith('/identity/password/reset/confirm')) {
        body = request.body;
        return http.Response(
          jsonEncode({'detail': 'Reset token expired'}),
          400,
          headers: {'content-type': 'application/json'},
        );
      }
      return http.Response('{}', 200);
    });

    final auth = AuthService();
    final changed = await auth.confirmPasswordReset(
      token: 'stale-code',
      newPassword: 'a long enough password',
    );

    expect(changed, isFalse);
    expect(auth.lastResetError, 'Reset token expired');
    expect(jsonDecode(body!)['reset_token'], 'stale-code');
  });

  test('a set password reports success and sends both values', () async {
    String? body;
    ApiClient.transport = MockClient((request) async {
      if (request.url.path.endsWith('/identity/password/reset/confirm')) {
        body = request.body;
        return http.Response(
          jsonEncode({'ok': true, 'message': 'Password reset successfully'}),
          200,
          headers: {'content-type': 'application/json'},
        );
      }
      return http.Response('{}', 200);
    });

    final auth = AuthService();
    final changed = await auth.confirmPasswordReset(
      token: 'good-code',
      newPassword: 'a long enough password',
    );

    expect(changed, isTrue);
    expect(jsonDecode(body!)['new_password'], 'a long enough password');
  });

  test('a pasted link gives up the code inside it', () {
    expect(
      AuthService.resetTokenFrom(
        'https://vimbai.example/reset/confirm?token=AbC-123_xyz',
      ),
      'AbC-123_xyz',
    );
    expect(
      AuthService.resetTokenFrom(
        'https://vimbai.example/reset/confirm/reset_token/Qq7',
      ),
      'Qq7',
    );
    expect(AuthService.resetTokenFrom('  bare-code-42  '), 'bare-code-42');
    expect(AuthService.resetTokenFrom(''), '');
  });

  testWidgets('the sign-in page leads to the reset flow', (tester) async {
    ApiClient.transport = MockClient((request) async {
      if (request.url.path.endsWith('/identity/password/reset')) {
        return http.Response(
          jsonEncode({
            'message':
                'If that address belongs to an account, a reset code is now valid for 24 hours and is recorded in this '
                "server's audit trail, because this Vimbai server has no email delivery yet. Ask whoever runs it for the "
                'code.',
          }),
          200,
          headers: {'content-type': 'application/json'},
        );
      }
      return http.Response('{}', 200);
    });

    await tester.pumpWidget(const MaterialApp(home: LoginPage()));
    expect(
      find.text('Forgot your password?'),
      findsOneWidget,
      reason:
          'a person who cannot sign in must have a way forward from that screen',
    );

    await tester.tap(find.text('Forgot your password?'));
    await tester.pumpAndSettle();
    expect(find.byType(PasswordResetPage), findsOneWidget);

    await tester.enterText(
      find.widgetWithText(TextField, 'Email address'),
      'sam@example.com',
    );
    await tester.tap(find.text('Ask for a reset code'));
    await tester.pumpAndSettle();

    expect(
      find.textContaining('If that address belongs to an account'),
      findsOneWidget,
      reason:
          'the engine answer is shown as it is, conditional wording included',
    );
    expect(
      find.textContaining('no email delivery yet'),
      findsOneWidget,
      reason:
          'and the sentence saying where the code really is reaches the person, so nobody waits for an email that '
          'cannot come',
    );
    expect(
      find.text('Reset code or link'),
      findsOneWidget,
      reason:
          'the second step is offered, because the code is what the person now holds',
    );
  });
}
