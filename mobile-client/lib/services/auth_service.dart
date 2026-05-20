import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:finacc_mobile_client/local_db/user_local_data.dart';
import 'package:finacc_mobile_client/models/user.dart';
import 'package:finacc_mobile_client/config.dart'; // Import the new config file

class AuthService {
  final String _baseUrl = AppConfig.apiUrl; // Use the URL from AppConfig

  Future<bool> register(String username, String email, String password) async {
    final connectivityResult = await (Connectivity().checkConnectivity());
    if (connectivityResult == ConnectivityResult.none) {
      // Offline registration: Store locally and queue for sync
      // In a real app, this would involve a proper local queue and a mechanism
      // to retry when online. For now, we'll simulate.
      print('Offline registration attempt: $username. Will sync later.');
      // Simulate saving a user locally for offline use (without full validation)
      final tempUser = User(id: 'temp-${DateTime.now().millisecondsSinceEpoch}', username: username, email: email, role: 'ACCOUNTANT');
      await UserLocalData.saveUser(tempUser);
      return true;
    } else {
      // Online registration
      final response = await http.post(
        Uri.parse('$_baseUrl/register'),
        headers: {'Content-Type': 'application/json'},
        body: json.encode({'username': username, 'email': email, 'password': password}),
      );

      if (response.statusCode == 201) {
        final data = json.decode(response.body);
        // Assuming the backend returns basic user info on registration
        final user = User(
          id: data['user_id'] ?? 'unknown', 
          username: username, 
          email: email, 
          role: 'ACCOUNTANT' // Default role for newly registered
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
    final connectivityResult = await (Connectivity().checkConnectivity());

    // 1. Try offline login first
    if (connectivityResult == ConnectivityResult.none) {
      final localUser = await UserLocalData.getUser();
      if (localUser != null && localUser.username == username) {
        print('Offline login successful for $username.');
        // In a real scenario, we'd need a local password hash or biometric
        // for full offline auth. For this POC, username match is sufficient.
        return true;
      }
      print('Offline login failed for $username. No local user found or mismatch.');
      return false; // No local user or credentials don't match (simplified)
    }

    // 2. If online, try remote login
    final response = await http.post(
      Uri.parse('$_baseUrl/login'),
      headers: {'Content-Type': 'application/json'},
      body: json.encode({'username': username, 'password': password}),
    );

    if (response.statusCode == 200) {
      final data = json.decode(response.body);
      final token = data['token'];

      // In a real app, the token needs to be parsed to get user details for offline caching.
      // For this POC, we'll simulate saving a generic user.
      final user = User(id: 'online-user-id', username: username, email: '$username@example.com', role: 'ACCOUNTANT'); // User ID will come from JWT payload later

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
    return user != null && token != null; // Simplified check
  }
}
