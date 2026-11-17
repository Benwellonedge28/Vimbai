// What a stranger sees when the password is wrong.
//
// The reason lived in a snackbar, so it slid away while the person was still reading it and left a form that looked as
// if the Login tap had done nothing at all. The reason now stays under the button until the next attempt.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/login_page.dart';

void main() {
  final originalTransport = ApiClient.transport;

  setUp(() => SharedPreferences.setMockInitialValues({}));
  tearDown(() => ApiClient.transport = originalTransport);

  Future<void> signIn(WidgetTester tester) async {
    await tester.pumpWidget(const MaterialApp(home: LoginPage()));
    await tester.enterText(find.byType(TextField).at(0), 'stranger');
    await tester.enterText(find.byType(TextField).at(1), 'the wrong password');
    await tester.tap(find.widgetWithText(ElevatedButton, 'Login'));
    await tester.pumpAndSettle();
  }

  testWidgets('a refused password says why, and the sentence stays', (
    tester,
  ) async {
    ApiClient.transport = MockClient((request) async {
      return http.Response(
        jsonEncode({
          'detail': 'Invalid username or password',
          'error': 'Invalid username or password',
        }),
        401,
        headers: {'content-type': 'application/json'},
      );
    });

    await signIn(tester);

    // no snackbar at all: the reason is part of the page
    expect(find.byType(SnackBar), findsNothing);
    expect(find.byKey(const Key('sign-in-refusal')), findsOneWidget);
    expect(
      find.textContaining('That username and password were not accepted'),
      findsOneWidget,
    );

    // it is still there a while later, which a snackbar would not be
    await tester.pump(const Duration(seconds: 10));
    expect(
      find.textContaining('That username and password were not accepted'),
      findsOneWidget,
    );
  });

  testWidgets('an unreachable engine is named as that, not as a wrong password', (
    tester,
  ) async {
    // The old sentence blamed the password whatever had happened. A phone with no engine running said "check your
    // username and password" to someone whose password was right.
    ApiClient.transport = MockClient((request) async {
      throw const SocketException('Connection refused');
    });

    await signIn(tester);

    expect(find.byKey(const Key('sign-in-refusal')), findsOneWidget);
    expect(find.textContaining('could not be reached'), findsOneWidget);
    expect(find.textContaining('were not accepted'), findsNothing);
  });
}
