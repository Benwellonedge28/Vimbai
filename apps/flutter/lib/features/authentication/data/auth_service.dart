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

  /// Why the last sign-in or registration was refused, in words a person can act on.
  ///
  /// The page used to say one thing for every cause - wrong password, an account wanting a second factor, an engine
  /// that was not running - so a person with the right password kept retyping it. Nothing technical goes in here: it is
  /// read aloud by the screen.
  String? lastError;

  /// True when the password was right but the account owes a second sign-in step, i.e. the sign-in is half done.
  ///
  /// The short-lived token the engine handed back lives only here: it is a challenge, not a session, and must never be
  /// stored, because a stored challenge is a way in waiting to be spent.
  bool awaitingSecondFactor = false;
  String? _pendingSecondFactorToken;
  String? secondFactorMethod;
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
      lastError =
          'The accounting engine could not be reached, so no account was created. Check that it is running, then try again.';
      return false;
    }

    if (response.statusCode == 201) {
      // An account, not a session: the engine answers with the new account and no tokens, so nothing is stored here and
      // the person signs in next. Storing a profile with no session was a half-signed-in state - a name on the phone
      // that no token backed - and it is what let an old build treat a cached profile as enough to get in.
      lastError = null;
      return true;
    }

    if (response.statusCode == 429) {
      final wait = _retryAfter(response.headers['retry-after'], response.body);
      lastError = wait == null
          ? 'Too many attempts. Wait a few minutes and try again.'
          : wait >= 120
          ? 'Too many attempts. Try again in about ${(wait / 60).ceil()} minutes.'
          : 'Too many attempts. Try again in $wait seconds.';
      return false;
    }

    if (response.statusCode == 400) {
      // the engine names the collision in words a person can act on: "Username already taken", "Email already
      // registered", or which password rule was missed
      lastError =
          _engineMessage(response.body) ??
          'That account cannot be created as entered. Try a different username or email.';
      return false;
    }

    print('Registration failed: ${response.statusCode} - ${response.body}');
    lastError = 'Registration failed. Try again.';
    return false;
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
      lastError =
          'The accounting engine could not be reached. Check that it is running, then try again.';
      return false;
    }

    if (response.statusCode == 429) {
      // the engine states the pause in seconds; repeating its own figure beats guessing
      final wait = _retryAfter(response.headers['retry-after'], response.body);
      lastError = wait == null
          ? 'Too many wrong attempts. Wait a few minutes and try again.'
          : wait >= 120
          ? 'Too many wrong attempts. Try again in about ${(wait / 60).ceil()} minutes.'
          : 'Too many wrong attempts. Try again in $wait seconds.';
      return false;
    }

    if (response.statusCode == 403) {
      lastError =
          'That account is not active. Ask an administrator to enable it.';
      return false;
    }

    if (response.statusCode != 200) {
      print('Online login failed: ${response.statusCode} - ${response.body}');
      lastError = 'That username and password were not accepted.';
      return false;
    }

    final data = json.decode(response.body) as Map<String, dynamic>;

    if (data['mfa_required'] == true) {
      // The password was right and the account owes its second step. The engine answers with a short-lived challenge
      // token; it is held in memory only and spent once, by completeSecondFactor.
      final challenge = data['temp_token'] as String?;
      if (challenge == null || challenge.isEmpty) {
        lastError =
            'The engine asked for a second step but sent nothing to complete it with. Try again.';
        return false;
      }
      awaitingSecondFactor = true;
      _pendingSecondFactorToken = challenge;
      secondFactorMethod = data['method'] as String?;
      lastError = secondFactorMethod == 'totp' || secondFactorMethod == null
          ? 'Enter the 6-digit code from your authenticator app.'
          : 'Enter the code this account asked for ($secondFactorMethod).';
      return false;
    }

    return _keepSession(data, username);
  }

  /// Finish a half-done sign-in with the code the account asked for.
  ///
  /// The code is checked by the engine that raised it, and a code that is wrong, reused or past its life is refused
  /// there. Nothing about the refusal is kept beyond the words for the person: a challenge that failed is spent.
  Future<bool> completeSecondFactor(String code) async {
    final challenge = _pendingSecondFactorToken;
    if (challenge == null) {
      lastError = 'Sign in with your password first.';
      return false;
    }
    final http.Response response;
    try {
      response = await _client.post(
        Uri.parse('$_baseUrl/identity/users/login/verify-mfa'),
        headers: {'Content-Type': 'application/x-www-form-urlencoded'},
        body:
            'temp_token=${Uri.encodeQueryComponent(challenge)}'
            '&code=${Uri.encodeQueryComponent(code.trim())}',
      );
    } on OfflineException catch (failure) {
      print('Second step refused: the engine could not be reached ($failure).');
      lastError =
          'The accounting engine could not be reached. Check that it is running, then try again.';
      return false;
    }

    if (response.statusCode == 429) {
      final wait = _retryAfter(response.headers['retry-after'], response.body);
      lastError = wait == null
          ? 'Too many attempts. Wait a few minutes and try again.'
          : 'Too many attempts. Try again in $wait seconds.';
      return false;
    }

    if (response.statusCode != 200) {
      // the engine names only "Invalid MFA code": whether the code was wrong or already spent is not the client's to know
      lastError =
          _engineMessage(response.body) ??
          'That code was not accepted. Check it and try again.';
      if (response.statusCode == 401) {
        _pendingSecondFactorToken = null;
        awaitingSecondFactor = false;
        lastError =
            'That code was not accepted, and it cannot be tried twice. Sign in again to get a new one.';
      }
      return false;
    }

    awaitingSecondFactor = false;
    _pendingSecondFactorToken = null;
    final data = json.decode(response.body) as Map<String, dynamic>;
    return _keepSession(data, data['username'] as String? ?? '');
  }

  /// The engine's own sentence about the last reset step, when it had one.
  ///
  /// A reset has two halves that fail for different reasons - an address it will not discuss, a link that has expired, a new
  /// password that misses a rule - and each has one thing the person can do next. The engine already says which; this holds
  /// it so the screen can read it out instead of composing a guess.
  String? lastResetError;

  /// What the engine answered when a reset was asked for. Its wording is conditional on purpose ("If the email exists...")
  /// and is shown as it stands: saying "we sent it" would turn this door into a way of discovering which addresses exist.
  String? lastResetMessage;

  /// Ask the engine to send a password reset for an address.
  Future<bool> requestPasswordReset(String email) async {
    lastResetError = null;
    lastResetMessage = null;
    final http.Response response;
    try {
      response = await _client.post(
        Uri.parse('$_baseUrl/identity/password/reset'),
        headers: {'Content-Type': 'application/json'},
        body: json.encode({'email': email.trim()}),
      );
    } on OfflineException catch (failure) {
      print('Password reset refused: the engine was not reachable ($failure).');
      lastResetError =
          'The accounting engine could not be reached. Check that it is running, then try again.';
      return false;
    }
    if (response.statusCode != 200) {
      lastResetError =
          _engineMessage(response.body) ??
          'That request could not be sent. Try again.';
      return false;
    }
    lastResetMessage =
        _engineMessage(response.body) ??
        'If that address is registered, a password reset link has been sent to it.';
    return true;
  }

  /// Set a new password with the code from a reset link.
  ///
  /// The code is checked by the engine that issued it: one that is wrong, already spent or past its hour is refused there,
  /// and so is a new password that misses a rule, with its own words. A completed reset signs every other session out, so
  /// this phone signs in again afterwards rather than pretending it kept its session.
  Future<bool> confirmPasswordReset({
    required String token,
    required String newPassword,
  }) async {
    lastResetError = null;
    lastResetMessage = null;
    final http.Response response;
    try {
      response = await _client.post(
        Uri.parse('$_baseUrl/identity/password/reset/confirm'),
        headers: {'Content-Type': 'application/json'},
        body: json.encode({
          'reset_token': token.trim(),
          'new_password': newPassword,
        }),
      );
    } on OfflineException catch (failure) {
      print(
        'Password reset not confirmed: the engine was not reachable ($failure).',
      );
      lastResetError =
          'The accounting engine could not be reached. Check that it is running, then try again.';
      return false;
    }
    if (response.statusCode != 200) {
      lastResetError =
          _engineMessage(response.body) ??
          'That reset link was not accepted. Ask for a new one.';
      return false;
    }
    return true;
  }

  /// The code inside a reset link, or the code on its own when that is what was pasted.
  ///
  /// A person copies what their mail client gives them, which is a whole address with the code buried in it. Digging it out
  /// here means the screen asks for one thing (paste the link) instead of asking a person to find the token in it.
  static String resetTokenFrom(String pasted) {
    final text = pasted.trim();
    if (text.isEmpty) return '';
    final uri = Uri.tryParse(text);
    if (uri != null && uri.queryParameters.isNotEmpty) {
      for (final name in const ['token', 'reset_token', 'code']) {
        final value = uri.queryParameters[name];
        if (value != null && value.isNotEmpty) return value;
      }
      final first = uri.queryParameters.values.first;
      if (first.isNotEmpty) return first;
    }
    // a link with the code as its last path segment, e.g. .../reset/AbC123
    if (text.contains('/') && !text.contains(' ')) {
      final last = text.split('/').where((part) => part.isNotEmpty).lastOrNull;
      if (last != null && !last.contains(':')) return last;
    }
    return text;
  }

  /// Store what a completed sign-in came back with: the tokens, when they expire, and who the person is.
  ///
  /// One place, because two doors now lead here - a password alone, and a password plus a code - and a second copy of
  /// this is how one of them quietly stops keeping the refresh token.
  Future<bool> _keepSession(Map<String, dynamic> data, String username) async {
    final token = data['access_token'] as String?;
    if (token == null) {
      print('Login response missing access_token.');
      lastError = 'The engine answered without a session. Try again.';
      return false;
    }
    lastError = null;
    await UserLocalData.saveAuthToken(token);
    final refresh = data['refresh_token'] as String?;
    if (refresh != null && refresh.isNotEmpty) {
      await UserLocalData.saveRefreshToken(refresh);
    }
    final seconds = (data['expires_in'] as num?)?.toInt();
    if (seconds != null) {
      await UserLocalData.saveAuthExpiry(
        DateTime.now().add(Duration(seconds: seconds)),
      );
    }

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

  /// The engine's own refusal message, which is written for a person to read.
  static String? _engineMessage(String body) {
    try {
      final data = json.decode(body);
      if (data is! Map) return null;
      // `message` is the engine's way of saying something to a person ("If the email exists...", "Password reset
      // successfully"); `error`/`detail` are its refusals. All three are sentences meant to be read aloud.
      final message = data['error'] ?? data['detail'] ?? data['message'];
      return message is String && message.isNotEmpty ? message : null;
    } on FormatException {
      return null;
    }
  }

  /// The pause the engine stated, in seconds, from its header or its body.
  static int? _retryAfter(String? header, String body) {
    final stated = int.tryParse((header ?? '').trim());
    if (stated != null) return stated;
    try {
      final data = json.decode(body);
      if (data is! Map) return null;
      final seconds = data['retry_after'];
      return seconds is num ? seconds.round() : null;
    } on FormatException {
      return null;
    }
  }

  /// Buy a new access token with the refresh token, once, when the old one is nearly spent.
  ///
  /// The access token lasts an hour and the app used to throw the refresh token away, so a phone worked when it was set
  /// up and refused everything the next morning while still calling itself signed in. Nothing about being offline is
  /// handled here: an engine that cannot be reached leaves the session as it is and the next call tries again.
  ///
  /// Returns true when the session now holds a fresh token. A refresh token the engine has already spent (or refused)
  /// means the session is over, so the stored session is cleared rather than left to fail on every screen.
  Future<bool> refreshSession({Duration? renewWithin}) async {
    final pending = _refreshing;
    if (pending != null) return pending;
    final started = _doRefresh(renewWithin: renewWithin);
    _refreshing = started;
    try {
      return await started;
    } finally {
      _refreshing = null;
    }
  }

  Future<bool>? _refreshing;

  /// Called when the engine refuses the refresh token, i.e. the session is genuinely over.
  ///
  /// The stored session is cleared either way; this is so the app can put the sign-in screen back. Without it the person
  /// sat on a home page whose every screen failed, with nothing saying why.
  static void Function()? onSessionEnded;

  Future<bool> _doRefresh({Duration? renewWithin}) async {
    final expiry = await UserLocalData.getAuthExpiry();
    final grace = renewWithin ?? const Duration(minutes: 5);
    final now = DateTime.now();
    if (expiry != null && expiry.isAfter(now.add(grace))) {
      return true; // still good, nothing to buy
    }
    final refresh = await UserLocalData.getRefreshToken();
    if (refresh == null || refresh.isEmpty) {
      // a session from before tokens were kept cannot be renewed; signing in again is the way back
      return false;
    }
    final http.Response response;
    try {
      response = await _client.post(
        Uri.parse('$_baseUrl/identity/token/refresh'),
        headers: {'Content-Type': 'application/x-www-form-urlencoded'},
        body: 'refresh_token=${Uri.encodeQueryComponent(refresh)}',
      );
    } on OfflineException {
      return false; // the engine could not be asked; keep what we have and try again later
    }
    if (response.statusCode != 200) {
      await UserLocalData.clearUserData();
      SyncService.instance.stop();
      onSessionEnded?.call();
      return false;
    }
    final data = json.decode(response.body) as Map<String, dynamic>;
    final token = data['access_token'] as String?;
    if (token == null) return false;
    await UserLocalData.saveAuthToken(token);
    final renewed = data['refresh_token'] as String?;
    if (renewed != null && renewed.isNotEmpty) {
      await UserLocalData.saveRefreshToken(renewed);
    }
    final seconds = (data['expires_in'] as num?)?.toInt();
    await UserLocalData.saveAuthExpiry(
      DateTime.now().add(Duration(seconds: seconds ?? 3600)),
    );
    return true;
  }

  /// Change this person's own password, with the current one as proof.
  ///
  /// The engine retires every refresh token the person holds when the password changes - that is the whole point of the moment
  /// - so this phone's session cannot be renewed afterwards. It is ended here rather than left to fail an hour later on some
  /// other screen: a change is finished with a fresh sign-in, which is also what the other devices now have to do.
  Future<bool> changePassword({
    required String currentPassword,
    required String newPassword,
  }) async {
    lastResetError = null;
    final user = await UserLocalData.getUser();
    final token = await UserLocalData.getAuthToken();
    if (user == null || token == null) {
      lastResetError = 'Sign in first.';
      return false;
    }
    final http.Response response;
    try {
      response = await _client.post(
        Uri.parse('$_baseUrl/identity/users/${user.id}/change-password'),
        headers: {
          'Content-Type': 'application/json',
          'Authorization': 'Bearer $token',
        },
        body: json.encode({
          'current_password': currentPassword,
          'new_password': newPassword,
        }),
      );
    } on OfflineException catch (failure) {
      print(
        'Password change not sent: the engine was not reachable ($failure).',
      );
      lastResetError =
          'The accounting engine could not be reached. Check that it is running, then try again.';
      return false;
    }
    if (response.statusCode != 200) {
      // "Current password is incorrect", or the rule the new one missed, in the engine's own words
      lastResetError =
          _engineMessage(response.body) ??
          'That password could not be changed. Try again.';
      return false;
    }
    await logout();
    return true;
  }

  Future<void> logout() async {
    await UserLocalData.clearUserData();
    SyncService.instance.stop(); // halt the offline outbox until next login
  }

  Future<bool> isLoggedIn() async {
    final user = await UserLocalData.getUser();
    final token = await UserLocalData.getAuthToken();
    if (user == null || token == null) return false;
    return refreshSession();
  }
}
