// Turning the second factor on and off from the phone.
//
// The point of these tests is that the second factor is finally satisfiable by a real person: the key comes from the engine,
// the person's own app makes the digits, and those digits are what the engine accepts. A screen that showed a key the engine
// never kept, or that asked for a code it had raised into an audit record nobody can read, would look identical on the
// surface and would still lock everyone out.
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
import 'package:vimbai_mobile_client/features/authentication/presentation/two_step_settings_page.dart';

final _user = {
  'id': 'user-1',
  'username': 'sam',
  'email': 'sam@example.com',
  'role': 'owner',
};

void main() {
  final originalTransport = ApiClient.transport;

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    await UserLocalData.saveUser(User.fromJson(_user));
    await UserLocalData.saveAuthToken('a-token');
  });

  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  test('the enrolment answer carries the key and the address an app reads', () async {
    ApiClient.transport = MockClient((request) async {
      expect(request.url.path, endsWith('/identity/users/user-1/mfa/setup'));
      expect(request.headers['Authorization'], 'Bearer a-token');
      // the method goes with the request: the engine must not guess what kind of factor this is
      expect(jsonDecode(request.body)['method'], 'totp');
      return http.Response(
        jsonEncode({
          'mfa_method': 'totp',
          'secret': 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ',
          'provisioning_uri':
              'otpauth://totp/Vimbai:sam%40example.com?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Vimbai',
          'code_sent': false,
          'message':
              'Add this key to your authenticator app, then enter the six digits it shows.',
        }),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    final enrolment = await AuthService().beginSecondFactor();

    expect(enrolment, isNotNull);
    expect(enrolment!.secret, 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ');
    expect(enrolment.provisioningUri, startsWith('otpauth://totp/'));
    expect(enrolment.message, contains('authenticator'));
  });

  test(
    'an engine that refuses hands back its reason, not an empty key',
    () async {
      ApiClient.transport = MockClient((request) async {
        return http.Response(
          jsonEncode({'detail': 'User not found'}),
          404,
          headers: {'content-type': 'application/json'},
        );
      });

      final auth = AuthService();
      expect(await auth.beginSecondFactor(), isNull);
      expect(auth.lastResetError, 'User not found');
    },
  );

  test('an unreachable engine is named as such', () async {
    ApiClient.transport = MockClient((request) async {
      throw http.ClientException('connection refused');
    });

    final auth = AuthService();
    expect(await auth.beginSecondFactor(), isNull);
    expect(auth.lastResetError, contains('could not be reached'));
  });

  test('confirming and turning off send the code to the right step', () async {
    final seen = <String>[];
    ApiClient.transport = MockClient((request) async {
      seen.add('${request.url.path}|${jsonDecode(request.body)['code']}');
      return http.Response(
        jsonEncode({'ok': true, 'message': 'MFA enabled successfully'}),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    final auth = AuthService();
    expect(await auth.confirmSecondFactor('123456'), isTrue);
    expect(await auth.disableSecondFactor('654321'), isTrue);
    expect(seen, [
      '/identity/users/user-1/mfa/verify|123456',
      '/identity/users/user-1/mfa/disable|654321',
    ]);
  });

  test('a refused code is reported in the engine words', () async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode({'detail': 'Invalid MFA code'}),
        400,
        headers: {'content-type': 'application/json'},
      );
    });

    final auth = AuthService();
    expect(await auth.confirmSecondFactor('000000'), isFalse);
    expect(auth.lastResetError, 'Invalid MFA code');
  });

  test('the account answer says whether the factor is on', () async {
    ApiClient.transport = MockClient((request) async {
      expect(request.url.path, endsWith('/identity/users/user-1'));
      return http.Response(
        jsonEncode({'id': 'user-1', 'username': 'sam', 'mfa_enabled': true}),
        200,
        headers: {'content-type': 'application/json'},
      );
    });
    expect(await AuthService().secondFactorEnabled(), isTrue);

    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode({'id': 'user-1', 'username': 'sam', 'mfa_enabled': false}),
        200,
        headers: {'content-type': 'application/json'},
      );
    });
    expect(await AuthService().secondFactorEnabled(), isFalse);
  });

  testWidgets('turning it on shows the key, then takes the digits', (
    tester,
  ) async {
    String? confirmed;
    ApiClient.transport = MockClient((request) async {
      final path = request.url.path;
      if (path.endsWith('/identity/users/user-1')) {
        return http.Response(
          jsonEncode({'id': 'user-1', 'mfa_enabled': false}),
          200,
          headers: {'content-type': 'application/json'},
        );
      }
      if (path.endsWith('/mfa/setup')) {
        return http.Response(
          jsonEncode({
            'mfa_method': 'totp',
            'secret': 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ',
            'provisioning_uri':
                'otpauth://totp/Vimbai:sam?secret=GEZDGNBVGY3TQOJQ',
            'message': 'Add this key to your authenticator app.',
          }),
          200,
          headers: {'content-type': 'application/json'},
        );
      }
      if (path.endsWith('/mfa/verify')) {
        confirmed = jsonDecode(request.body)['code'] as String?;
        return http.Response(
          jsonEncode({'ok': true, 'message': 'MFA enabled successfully'}),
          200,
          headers: {'content-type': 'application/json'},
        );
      }
      return http.Response('{}', 200);
    });

    await tester.pumpWidget(
      MaterialApp(home: TwoStepSettingsPage(authService: AuthService())),
    );
    await tester.pumpAndSettle();

    expect(
      find.textContaining('Two-step sign-in is off'),
      findsOneWidget,
      reason: 'the screen opens by saying what the account does now',
    );

    await tester.tap(find.text('Turn on two-step sign-in'));
    await tester.pumpAndSettle();

    expect(
      find.text('GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ'),
      findsOneWidget,
      reason:
          'the key is shown as text: the phone showing a QR code cannot scan it',
    );
    expect(find.textContaining('otpauth://totp/'), findsOneWidget);

    await tester.enterText(
      find.widgetWithText(TextField, 'Six digits from your app'),
      '123456',
    );
    await tester.tap(find.text('Turn it on'));
    await tester.pumpAndSettle();

    expect(confirmed, '123456');
    expect(
      find.textContaining('Next time you sign in'),
      findsOneWidget,
      reason:
          'the person is told what changed, in the words they will meet at the door',
    );
    expect(
      find.text('GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ'),
      findsNothing,
      reason: 'the key is shown once; it is not left on the screen afterwards',
    );
  });

  testWidgets('turning it off asks for a code and says why', (tester) async {
    ApiClient.transport = MockClient((request) async {
      final path = request.url.path;
      if (path.endsWith('/identity/users/user-1')) {
        return http.Response(
          jsonEncode({'id': 'user-1', 'mfa_enabled': true}),
          200,
          headers: {'content-type': 'application/json'},
        );
      }
      if (path.endsWith('/mfa/disable')) {
        return http.Response(
          jsonEncode({'detail': 'Invalid MFA code'}),
          400,
          headers: {'content-type': 'application/json'},
        );
      }
      return http.Response('{}', 200);
    });

    await tester.pumpWidget(
      MaterialApp(home: TwoStepSettingsPage(authService: AuthService())),
    );
    await tester.pumpAndSettle();
    expect(find.textContaining('is on for this account'), findsOneWidget);

    await tester.enterText(
      find.widgetWithText(TextField, 'Six digits from your app'),
      '000000',
    );
    await tester.tap(find.text('Turn off two-step sign-in'));
    await tester.pumpAndSettle();

    expect(find.text('Invalid MFA code'), findsOneWidget);
    expect(
      find.textContaining('is on for this account'),
      findsOneWidget,
      reason:
          'a refused code must not leave the screen claiming the factor is off',
    );
  });
}
