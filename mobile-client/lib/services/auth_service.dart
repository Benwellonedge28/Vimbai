import 'dart:convert';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:vimbai_mobile_client/models/user.dart';
import 'package:vimbai_mobile_client/services/api_client.dart';
import 'package:vimbai_mobile_client/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/config.dart'; // For API URL

class AuthService {
  final ApiClient _client = ApiClient();
  // final String _baseUrl = AppConfig.apiUrl; // Old: Was identity service base URL
  final String _baseUrl = AppConfig.apiUrl; // NEW: Now it's the API Gateway URL

  /// Auth token for outgoing API requests (null when logged out).
  Future<String?> getToken() async {
    return await UserLocalData.getAuthToken();
  }

  Future<bool> register(String username, String email, String password, String roleName) async {
    final connectivityResults = await Connectivity().checkConnectivity();
    if (connectivityResults.contains(ConnectivityResult.none) || connectivityResults.isEmpty) {
      // Offline registration: Store locally and queue for sync
      print('Offline registration attempt: $username. Will sync later.');
      final tempUser = User(id: 'temp-${DateTime.now().millisecondsSinceEpoch}', username: username, email: email, role: roleName);
      await UserLocalData.saveUser(tempUser);
      return true;
    } else {
      // Online registration
      final response = await _client.post(
        // Uri.parse('$_baseUrl/register'), // Old: direct identity service
        Uri.parse('$_baseUrl/identity/register'), // NEW: via API Gateway
        headers: {'Content-Type': 'application/json'},
        body: json.encode({'username': username, 'email': email, 'password': password, 'role_name': roleName}),
      );

      if (response.statusCode == 201) {
        final data = json.decode(response.body);
        final user = User(
          id: data['user_id'] ?? 'unknown', 
          username: username, 
          email: email, 
          role: roleName
        );
        await UserLocalData.saveUser(user);
        return true;
      } else {
        print('Registration failed: ${response.body}');
        return false;
      }
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

    final response = await _client.post(
      // Uri.parse('$_baseUrl/login'), // Old: direct identity service
      Uri.parse('$_baseUrl/identity/login'), // NEW: via API Gateway
      headers: {'Content-Type': 'application/json'},
      body: json.encode({'username': username, 'password': password}),
    );

    if (response.statusCode == 200) {
      final data = json.decode(response.body);
      final token = data['token'];

      final user = User(id: 'online-user-id', username: username, email: '$username@example.com', role: 'ACCOUNTANT'); 

      await UserLocalData.saveUser(user);
      await UserLocalData.saveAuthToken(token);
      print('Online login successful for $username. Token saved.');
      return true;
    } else {
      print('Online login failed: ${response.body}');
      return false;
    }
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
