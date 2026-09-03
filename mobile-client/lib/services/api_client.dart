// lib/services/api_client.dart
// Offline-aware HTTP client wrapper.
//
// Every network call in the app goes through this client, which:
//   1. Applies a short timeout so the UI fails fast when offline
//      and can fall back to local SQLite data.
//   2. Maps low-level socket/timeout errors to a typed
//      [OfflineException] so pages can distinguish "no network"
//      from "server rejected the request".
//   3. Retries idempotent GET requests once on transient failures.

import 'dart:async';
import 'dart:io';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/config.dart';

/// Thrown when a request fails because the device is offline or the
/// server is unreachable. Callers should fall back to local data.
class OfflineException implements Exception {
  final String message;
  OfflineException([this.message = 'Device offline or server unreachable']);
  @override
  String toString() => message;
}

class ApiClient {
  static const Duration _timeout = AppConfig.requestTimeout;

  /// Wraps a future with the standard timeout and offline error mapping.
  static Future<http.Response> _guard(Future<http.Response> future,
      {bool retryOnFailure = false, int attempt = 0}) async {
    try {
      return await future.timeout(_timeout);
    } on TimeoutException {
      if (retryOnFailure && attempt < AppConfig.maxRetries) {
        return _guard(future, retryOnFailure: true, attempt: attempt + 1);
      }
      throw OfflineException('Request timed out');
    } on SocketException catch (e) {
      throw OfflineException('No network connection: ${e.message}');
    } on HttpException catch (e) {
      throw OfflineException(e.message);
    }
  }

  Future<http.Response> get(Uri url, {Map<String, String>? headers}) =>
      _guard(http.get(url, headers: headers), retryOnFailure: true);

  Future<http.Response> post(Uri url,
          {Map<String, String>? headers, Object? body}) =>
      _guard(http.post(url, headers: headers, body: body));

  Future<http.Response> put(Uri url,
          {Map<String, String>? headers, Object? body}) =>
      _guard(http.put(url, headers: headers, body: body));

  Future<http.Response> delete(Uri url, {Map<String, String>? headers}) =>
      _guard(http.delete(url, headers: headers));
}
