import 'dart:convert';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/features/authentication/models/user.dart';

class UserLocalData {
  static const _userKey = 'currentUser';
  static const _tokenKey = 'authToken';
  static const _refreshKey = 'refreshToken';
  static const _expiryKey = 'authTokenExpiresAt';

  static Future<void> saveUser(User user) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_userKey, json.encode(user.toJson()));
  }

  /// The ID of the locally stored (logged-in) user, if any.
  static Future<String?> getUserId() async {
    final user = await getUser();
    return user?.id;
  }

  static Future<User?> getUser() async {
    final prefs = await SharedPreferences.getInstance();
    final userJson = prefs.getString(_userKey);
    if (userJson != null) {
      return User.fromJson(json.decode(userJson));
    }
    return null;
  }

  static Future<void> saveAuthToken(String token) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_tokenKey, token);
  }

  static Future<String?> getAuthToken() async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.getString(_tokenKey);
  }

  /// The token that buys a new access token. Kept beside the access token, never in the same place as the password.
  static Future<void> saveRefreshToken(String token) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_refreshKey, token);
  }

  static Future<String?> getRefreshToken() async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.getString(_refreshKey);
  }

  /// When the access token stops working, as milliseconds since the epoch.
  ///
  /// The access token lasts an hour. Without this the app kept a dead token, still called itself signed in, and every
  /// screen wore the same "could not load" failure until the person worked out to sign out and back in.
  static Future<void> saveAuthExpiry(DateTime at) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setInt(_expiryKey, at.millisecondsSinceEpoch);
  }

  static Future<DateTime?> getAuthExpiry() async {
    final prefs = await SharedPreferences.getInstance();
    final millis = prefs.getInt(_expiryKey);
    return millis == null ? null : DateTime.fromMillisecondsSinceEpoch(millis);
  }

  static Future<void> clearUserData() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.remove(_userKey);
    await prefs.remove(_tokenKey);
    await prefs.remove(_refreshKey);
    await prefs.remove(_expiryKey);
  }
}
