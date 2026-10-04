import 'dart:convert';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:vimbai_mobile_client/models/user.dart';
import 'package:vimbai_mobile_client/services/api_client.dart';
import 'package:vimbai_mobile_client/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/config.dart'; // For API URL

/// Client for the identity-service via the API gateway.
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

  Future<bool> register(String username, String email, String password, String roleName) async {
    final connectivityResults = await Connectivity().checkConnectivity();
    if (connectivityResults.contains(ConnectivityResult.none) || connectivityResults.isEmpty) {
      // Offline registration: Store locally and queue for sync
      print('Offline registration attempt: $username. Will sync later.');
      final tempUser = User(
        id: 'temp-${DateTime.now().millisecondsSinceEpoch}',
        username: username,
        email: email,
        role: roleName,
      );
      await UserLocalData.saveUser(tempUser);
      return true;
    }

    // Online registration through the gateway. role_ids stays empty: the
    // identity service seeds default roles on startup and validates any
    // provided ids, so an unknown role name must not be sent.
    final response = await _client.post(
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
    final connectivityResults = await Connectivity().checkConnectivity();

    if (connectivityResults.contains(ConnectivityResult.none) || connectivityResults.isEmpty) {
      final localUser = await UserLocalData.getUser();
      if (localUser != null && localUser.username == username) {
        print('Offline login successful for $username.');
        return true;
      }
      print('Offline login failed for $username. No local user found or mismatch.');
      return false;
    }

    // OAuth2 password flow: the identity service expects form data, not JSON.
    final response = await _client.post(
      Uri.parse('$_baseUrl/identity/users/login'),
      headers: {'Content-Type': 'application/x-www-form-urlencoded'},
      body: 'username=${Uri.encodeQueryComponent(username)}'
          '&password=${Uri.encodeQueryComponent(password)}',
    );

    if (response.statusCode != 200) {
      print('Online login failed: ${response.statusCode} - ${response.body}');
      return false;
    }

    final data = json.decode(response.body) as Map<String, dynamic>;

    if (data['mfa_required'] == true) {
      // MFA is enabled for this account. The mobile client does not support
      // the second factor yet, so surface it instead of storing a dead token.
      print('MFA required (method: ${data['method']}); mobile client cannot complete this login.');
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
      final user = User(id: 'unknown', username: username, email: '$username@example.com', role: 'user');
      await UserLocalData.saveUser(user);
    }
    print('Online login successful for $username. Token saved.');
    return true;
  }

  Future<void> logout() async {
    await UserLocalData.clearUserData();
  }

  Future<bool> isLoggedIn() async {
    final user = await UserLocalData.getUser();
    final token = await UserLocalData.getAuthToken();
    return user != null && token != null;
  }
}
