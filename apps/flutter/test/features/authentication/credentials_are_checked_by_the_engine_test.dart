// A password is checked by the engine that holds the account, on every sign-in, whether or not the phone has a network.
//
// The class this covers once had an offline branch that admitted any username whose profile was already on the phone and
// fabricated an account for any registration. Both were walked through with a wrong password. These tests pin the answer
// to the engine instead, using a replaced transport so no network is involved.
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';

void main() {
  // The accepted-registration path saves the new account on the phone, which goes through shared_preferences.
  TestWidgetsFlutterBinding.ensureInitialized();
  setUp(() {
    SharedPreferences.setMockInitialValues({});
  });

  // Held lazily: reading the static transport builds an IOClient, which may only happen inside a test zone.
  late http.Client originalTransport;
  setUp(() {
    originalTransport = ApiClient.transport;
  });
  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  test('a wrong password is refused even when the engine says so', () async {
    ApiClient.transport = MockClient((request) async {
      expect(request.url.path, endsWith('/identity/users/login'));
      return http.Response('{"detail":"Incorrect username or password"}', 401);
    });

    final admitted = await AuthService().login('sam', 'not-the-password');

    expect(
      admitted,
      isFalse,
      reason: 'a refused password must not open the app',
    );
  });

  test(
    'an engine that cannot be reached refuses the sign-in rather than admitting anyone',
    () async {
      ApiClient.transport = MockClient((request) async {
        throw http.ClientException('connection refused', request.url);
      });

      final admitted = await AuthService().login('sam', 'anything');

      expect(admitted, isFalse);
    },
  );

  test(
    'registration asks the engine and does not make up an account when it cannot answer',
    () async {
      var asked = 0;
      ApiClient.transport = MockClient((request) async {
        asked++;
        throw http.ClientException('connection refused', request.url);
      });

      final made = await AuthService().register(
        'sam',
        'sam@example.com',
        'secret',
        'user',
      );

      expect(
        asked,
        1,
        reason: 'registration must ask the engine, not decide on its own',
      );
      expect(
        made,
        isFalse,
        reason: 'an account invented on the phone is not an account',
      );
    },
  );

  test('a registration the engine accepts is a registration', () async {
    ApiClient.transport = MockClient((request) async {
      expect(request.url.path, endsWith('/identity/users/register'));
      return http.Response(
        '{"id":"acct-1","username":"sam","email":"sam@example.com"}',
        201,
      );
    });

    final made = await AuthService().register(
      'sam',
      'sam@example.com',
      'secret',
      'user',
    );

    expect(made, isTrue);
  });
}
