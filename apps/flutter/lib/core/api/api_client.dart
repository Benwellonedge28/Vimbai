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
import 'package:vimbai_mobile_client/app/config.dart';

/// Thrown when a request fails because the device is offline or the
/// server is unreachable. Callers should fall back to local data.
class OfflineException implements Exception {
  final String message;
  OfflineException([this.message = 'Device offline or server unreachable']);
  @override
  String toString() => message;
}

class ApiClient {
  /// How long one attempt may take before it is abandoned.
  ///
  /// Overridable so a test does not have to wait the shipped fifteen seconds to prove what a timeout does.
  static Duration timeout = AppConfig.requestTimeout;

  /// The socket every call goes through.
  ///
  /// Held in one place so a test can hand the client a transport that answers without a network, and so the rules below -
  /// which requests are retried, what a timeout becomes - can be proven rather than assumed. Nothing in the app sets this.
  static http.Client transport = http.Client();

  /// Runs one attempt with the standard timeout and offline error mapping.
  ///
  /// It takes the call rather than a started future: a retry has to *ask again*, and a future that has already failed
  /// cannot be re-run. Handing over a future meant the second attempt re-waited on the first attempt's failure and gave up
  /// immediately - the retry existed in the code and never happened on the wire.
  static Future<http.Response> _guard(
    Future<http.Response> Function() send, {
    bool retryOnFailure = false,
    int attempt = 0,
  }) async {
    try {
      return await send().timeout(timeout);
    } on TimeoutException {
      if (retryOnFailure && attempt < AppConfig.maxRetries) {
        return _guard(send, retryOnFailure: true, attempt: attempt + 1);
      }
      throw OfflineException('Request timed out');
    } on SocketException catch (e) {
      // A dropped connection on a read is worth another ask; on a write it is not, because the server may have done the
      // work before the connection went.
      if (retryOnFailure && attempt < AppConfig.maxRetries) {
        return _guard(send, retryOnFailure: true, attempt: attempt + 1);
      }
      throw OfflineException('No network connection: ${e.message}');
    } on HttpException catch (e) {
      throw OfflineException(e.message);
    }
  }

  Future<http.Response> get(Uri url, {Map<String, String>? headers}) =>
      _guard(() => transport.get(url, headers: headers), retryOnFailure: true);

  Future<http.Response> post(Uri url,
          {Map<String, String>? headers, Object? body}) =>
      _guard(() => transport.post(url, headers: headers, body: body));

  Future<http.Response> put(Uri url,
          {Map<String, String>? headers, Object? body}) =>
      _guard(() => transport.put(url, headers: headers, body: body));

  Future<http.Response> delete(Uri url, {Map<String, String>? headers}) =>
      _guard(() => transport.delete(url, headers: headers));
}
