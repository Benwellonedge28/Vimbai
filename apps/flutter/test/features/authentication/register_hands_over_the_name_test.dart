import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/login_page.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/register_page.dart';

/// An engine that makes accounts without a phone.
class _Registering extends AuthService {
  _Registering({this.refusal});

  final String? refusal;
  final created = <String>[];

  @override
  Future<bool> register(
    String username,
    String email,
    String password,
    String roleName,
  ) async {
    if (refusal != null) {
      lastError = refusal;
      return false;
    }
    created.add(username);
    return true;
  }
}

void main() {
  testWidgets('a new account hands its name to sign-in', (tester) async {
    // An account is not a session: the engine answers with the account and no token, so signing in is the next step.
    // Making the person retype a name they invented a minute ago is friction with nothing behind it, so the name
    // arrives filled in and only the password they just chose is left to type.
    final service = _Registering();
    await tester.pumpWidget(
      MaterialApp(home: RegisterPage(authService: service)),
    );

    await tester.enterText(find.widgetWithText(TextField, 'Username'), 'tino');
    await tester.enterText(
      find.widgetWithText(TextField, 'Email'),
      'tino@example.com',
    );
    await tester.enterText(
      find.widgetWithText(TextField, 'Password'),
      'correct horse battery',
    );
    await tester.tap(find.widgetWithText(ElevatedButton, 'Register'));
    await tester.pumpAndSettle();

    expect(service.created, ['tino']);
    expect(find.byType(LoginPage), findsOneWidget);
    final name = tester.widget<TextField>(
      find.widgetWithText(TextField, 'Username'),
    );
    expect(name.controller?.text, 'tino', reason: 'the name carries over');
    final password = tester.widget<TextField>(
      find.widgetWithText(TextField, 'Password'),
    );
    expect(
      password.controller?.text,
      isEmpty,
      reason: 'the password is not carried across, and never was',
    );
    expect(
      find.text('Account created. Sign in to open your Books.'),
      findsOneWidget,
    );
  });

  testWidgets('a refused registration stays put and says why', (tester) async {
    final service = _Registering(refusal: 'That name is already taken');
    await tester.pumpWidget(
      MaterialApp(home: RegisterPage(authService: service)),
    );

    await tester.enterText(find.widgetWithText(TextField, 'Username'), 'tino');
    await tester.enterText(
      find.widgetWithText(TextField, 'Email'),
      'tino@example.com',
    );
    await tester.enterText(
      find.widgetWithText(TextField, 'Password'),
      'correct horse battery',
    );
    await tester.tap(find.widgetWithText(ElevatedButton, 'Register'));
    await tester.pumpAndSettle();

    expect(find.byType(LoginPage), findsNothing);
    expect(find.textContaining('already taken'), findsWidgets);
  });

  testWidgets('a password too short to be allowed is refused before any call', (
    tester,
  ) async {
    final service = _Registering();
    await tester.pumpWidget(
      MaterialApp(home: RegisterPage(authService: service)),
    );

    await tester.enterText(find.widgetWithText(TextField, 'Username'), 'tino');
    await tester.enterText(
      find.widgetWithText(TextField, 'Email'),
      'tino@example.com',
    );
    await tester.enterText(find.widgetWithText(TextField, 'Password'), 'short');
    await tester.tap(find.widgetWithText(ElevatedButton, 'Register'));
    await tester.pumpAndSettle();

    expect(service.created, isEmpty, reason: 'no account is asked for');
    expect(find.textContaining('at least 8 characters'), findsWidgets);
  });
}
