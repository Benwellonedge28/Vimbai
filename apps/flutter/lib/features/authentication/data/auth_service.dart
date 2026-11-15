import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/features/authentication/models/user.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/core/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/offline/sync_service.dart'; // For API URL

/// Client for the identity-service via the API gateway.
///
/// The engine decides who someone is, on every sign-in and every registration, whether or not the phone has a network.
/// On a device the engine runs locally, so "offline" is not a reason to stop asking it: the previous version of this
/// class read `connectivity_plus` and, with no connection, admitted any username whose profile was already on the phone
/// and fabricated an account for any registration. Both were gaps a person could walk through with a wrong password,
/// and neither produced a token, so the session could not read anything anyway.
///
/// Real contract (gateway strips the /identity prefix before proxying):
///   POST /identity/users/register  JSON  {email, username, password, ...}
///     -> 201 {id, email, username}
///   POST /identity/users/login     form  username, password
///     -> 200 {access_token, refresh_token, expires_in}
///        or 200 {mfa_required: true, method, temp_token}
///   GET  /identity/users/me        Bearer
///     -> the authenticated user's profile
class AuthService {
  final ApiClient _client = ApiClient();
  final String _baseUrl = AppConfig.apiUrl; // API Gateway URL

  /// Auth token for outgoing API requests (null when logged out).
  Future<String?> getToken() async {
    return await UserLocalData.getAuthToken();
  }

  Future<bool> register(
    String username,
    String email,
    String password,
    String roleName,
  ) async {
    // Registration goes to the engine. role_ids stays empty: the
    // identity service seeds default roles on startup and validates any
    // provided ids, so an unknown role name must not be sent.
    final http.Response response;
    try {
      response = await _client.post(
        Uri.parse('$_baseUrl/identity/users/register'),
        headers: {'Content-Type': 'application/json'},
        body: json.encode({
          'username': username,
          'email': email,
          'password': password,
          'first_name': username,
          'role_ids': <String>[],
        }),
      );
    } on OfflineException catch (failure) {
      // The engine could not be asked, so no account exists. Saying so is the honest answer: an account made up on the
      // phone is not an account, and letting the person in would mean a book they can never open on any other machine.
      print(
        'Registration refused: the accounting engine could not be reached ($failure).',
      );
      return false;
    }

    if (response.statusCode == 201) {
      final data = json.decode(response.body) as Map<String, dynamic>;
      final user = User(
        id: (data['id'] as String?) ?? 'unknown',
        username: (data['username'] as String?) ?? username,
        email: (data['email'] as String?) ?? email,
        role: roleName,
      );
      await UserLocalData.saveUser(user);
      return true;
    } else {
      print('Registration failed: ${response.body}');
      return false;
    }
  }

  Future<bool> login(String username, String password) async {
    // OAuth2 password flow: the identity service expects form data, not JSON.
    //
    // There is deliberately no "we look offline, so let them in" branch. The password is checked by the engine that
    // holds the account, which on a device is the engine running beside the app; a phone with no network and a live
    // local engine still signs in, and a phone that cannot reach its engine is told so rather than admitted.
    final http.Response response;
    try {
      response = await _client.post(
        Uri.parse('$_baseUrl/identity/users/login'),
        headers: {'Content-Type': 'application/x-www-form-urlencoded'},
        body:
            'username=${Uri.encodeQueryComponent(username)}'
            '&password=${Uri.encodeQueryComponent(password)}',
      );
    } on OfflineException catch (failure) {
      print(
        'Sign-in refused: the accounting engine could not be reached ($failure).',
      );
      return false;
    }

    if (response.statusCode != 200) {
      print('Online login failed: ${response.statusCode} - ${response.body}');
      return false;
    }

    final data = json.decode(response.body) as Map<String, dynamic>;

    if (data['mfa_required'] == true) {
      // MFA is enabled for this account. The mobile client does not support
      // the second factor yet, so surface it instead of storing a dead token.
      print(
        'MFA required (method: ${data['method']}); mobile client cannot complete this login.',
      );
      return false;
    }

    final token = data['access_token'] as String?;
    if (token == null) {
      print('Login response missing access_token.');
      return false;
    }
    await UserLocalData.saveAuthToken(token);

    // Fetch the real profile so the stored user has the server-side id
    // (the login response only carries tokens).
    final meResponse = await _client.get(
      Uri.parse('$_baseUrl/identity/users/me'),
      headers: {'Authorization': 'Bearer $token'},
    );
    if (meResponse.statusCode == 200) {
      final me = json.decode(meResponse.body) as Map<String, dynamic>;
      final user = User(
        id: (me['id'] as String?) ?? 'unknown',
        username: (me['username'] as String?) ?? username,
        email: (me['email'] as String?) ?? '$username@example.com',
        role: 'user',
      );
      await UserLocalData.saveUser(user);
    } else {
      // Token is valid; keep a minimal profile so the session works.
      final user = User(
        id: 'unknown',
        username: username,
        email: '$username@example.com',
        role: 'user',
      );
      await UserLocalData.saveUser(user);
    }
    print('Online login successful for $username. Token saved.');
    return true;
  }

  Future<void> logout() async {
    await UserLocalData.clearUserData();
    SyncService.instance.stop(); // halt the offline outbox until next login
  }

  Future<bool> isLoggedIn() async {
    final user = await UserLocalData.getUser();
    final token = await UserLocalData.getAuthToken();
    return user != null && token != null;
  }
}
