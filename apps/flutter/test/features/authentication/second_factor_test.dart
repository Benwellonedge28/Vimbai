// The second step of a sign-in.
//
// An account with a second factor used to be a dead end on a phone: the app said it could not do this and told the person
// to use another client. The engine has always accepted the code (login/verify-mfa); what was missing was the screen and
// the call. A challenge token is a way in waiting to be spent, so it is held in memory, never stored, and never reused.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/core/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/core/offline/sync_service.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/login_page.dart';
import 'package:vimbai_mobile_client/features/books/data/book_sync_service.dart';

void main() {
  final auth = AuthService();
  final originalTransport = ApiClient.transport;
  late Directory deviceDir;

  // a completed sign-in lands on the home page, which reads the phone's Book store: give this file its own directory so
  // it cannot fight another test file over the locks
  setUpAll(() {
    sqfliteFfiInit();
    databaseFactory = databaseFactoryFfi;
    deviceDir = Directory.systemTemp.createTempSync('vimbai_second_factor_');
    BookSyncService.databasesDirectory = deviceDir.path;
  });

  tearDownAll(() {
    BookSyncService.databasesDirectory = null;
    if (deviceDir.existsSync()) deviceDir.deleteSync(recursive: true);
  });

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    await UserLocalData.clearUserData();
  });

  tearDown(() => ApiClient.transport = originalTransport);

  /// An engine that wants a code first, then accepts one specific code.
  MockClient engine({required String accepting, int failures = 0}) {
    var refused = 0;
    return MockClient((request) async {
      if (request.url.path.endsWith('/users/login')) {
        return http.Response(
          jsonEncode({
            'mfa_required': true,
            'method': 'totp',
            'temp_token': 'ch-1',
          }),
          200,
        );
      }
      if (request.url.path.endsWith('/login/verify-mfa')) {
        final code = Uri.splitQueryString(request.body)['code'];
        if (code != accepting || refused < failures) {
          refused++;
          return http.Response(
            '{"detail":"Invalid MFA code","error":"Invalid MFA code"}',
            401,
          );
        }
        return http.Response(
          jsonEncode({
            'access_token': 'access-after-code',
            'refresh_token': 'refresh-after-code',
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
  }

  test('the password starting a second step is not a failed sign-in', () async {
    ApiClient.transport = engine(accepting: '123456');
    expect(await auth.login('sam', 'right'), isFalse);
    expect(auth.awaitingSecondFactor, isTrue);
    expect(auth.secondFactorMethod, 'totp');
    expect(
      await UserLocalData.getAuthToken(),
      isNull,
      reason: 'a challenge is not a session',
    );
  });

  test(
    'the code finishes the sign-in and the session is kept with its refresh',
    () async {
      ApiClient.transport = engine(accepting: '123456');
      await auth.login('sam', 'right');
      expect(await auth.completeSecondFactor('123456'), isTrue);
      expect(auth.awaitingSecondFactor, isFalse);
      expect(await UserLocalData.getAuthToken(), 'access-after-code');
      expect(await UserLocalData.getRefreshToken(), 'refresh-after-code');
      expect(await UserLocalData.getUser(), isNotNull);
    },
  );

  test(
    'a wrong code costs the challenge, so it cannot be tried twice',
    () async {
      ApiClient.transport = engine(accepting: '123456');
      await auth.login('sam', 'right');
      expect(await auth.completeSecondFactor('000000'), isFalse);
      expect(auth.awaitingSecondFactor, isFalse);
      expect(auth.lastError, contains('Sign in again'));

      // the spent challenge is not silently retried against the engine
      expect(await auth.completeSecondFactor('123456'), isFalse);
      expect(auth.lastError, contains('Sign in with your password first'));
    },
  );

  test('a code with no password before it is refused here, not sent', () async {
    expect(await auth.completeSecondFactor('123456'), isFalse);
    expect(auth.lastError, contains('Sign in with your password first'));
  });

  testWidgets(
    'the sign-in screen leads to the code screen, and the code signs in',
    (tester) async {
      ApiClient.transport = engine(accepting: '123456');

      await tester.pumpWidget(const MaterialApp(home: LoginPage()));
      await tester.enterText(find.byType(TextField).at(0), 'sam');
      await tester.enterText(find.byType(TextField).at(1), 'right');
      await tester.tap(find.widgetWithText(ElevatedButton, 'Login'));
      await tester.pumpAndSettle();

      expect(find.text('Second step'), findsOneWidget);
      expect(find.textContaining('authenticator app'), findsOneWidget);

      await tester.enterText(find.byType(TextField).first, '123456');
      await tester.tap(find.widgetWithText(ElevatedButton, 'Verify'));
      await tester.pumpAndSettle();

      expect(await UserLocalData.getAuthToken(), 'access-after-code');

      // the home page starts the sync timer; a widget test has to put the tree away or the timer outlives it
      SyncService.instance.stop();
      await tester.pumpWidget(const SizedBox());
      await tester.pump();
    },
  );

  testWidgets(
    'a wrong code is shown on the code screen and the person is sent back',
    (tester) async {
      ApiClient.transport = engine(accepting: '123456');

      await tester.pumpWidget(const MaterialApp(home: LoginPage()));
      await tester.enterText(find.byType(TextField).at(0), 'sam');
      await tester.enterText(find.byType(TextField).at(1), 'right');
      await tester.tap(find.widgetWithText(ElevatedButton, 'Login'));
      await tester.pumpAndSettle();

      await tester.enterText(find.byType(TextField).first, '000000');
      await tester.tap(find.widgetWithText(ElevatedButton, 'Verify'));
      await tester.pumpAndSettle();

      // back to sign in, with nothing stored and no half session
      expect(find.text('Vimbai Login'), findsOneWidget);
      expect(await UserLocalData.getAuthToken(), isNull);
    },
  );
}
