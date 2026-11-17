import 'dart:convert';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/core/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/features/banking/models/banking_models.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// Client for the bank feed via the API gateway.
///
/// Every address is the engine's own (all Book-scoped by the gateway's X-Book-ID injection), because the retired
/// banking-integration service's addresses - `/connect`, `/sync/{id}`, `/transactions/{id}` - are not answered by
/// anything: they were the one surface where the app's screens had no working address under them at all.
///   POST   /bank-feed/connections               query: bank_name, account_number,
///                                               api_key, account_type
///   GET    /bank-feed/connections
///   POST   /bank-feed/sync/start                body: {bank_connection_id}
///   GET    /bank-feed/transactions              query: connection_id, limit, offset
///   POST   /bank-feed/transactions/{id}/reconcile
///          body: {matched_entry_id?, notes?}
///   DELETE /bank-feed/connections/{connection_id}
class BankingApiService {
  final ApiClient _client = ApiClient();
  final String _bankingServiceUrl = AppConfig.bankingRoute;

  Future<Map<String, String>> _getHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer $token',
      ...BookContext.instance.headers(),
    };
  }

  /// Establish a bank connection.
  /// Record a bank account by hand.
  ///
  /// No secret is asked for and none is kept: a hand-added account is the engine's `manual` feed, which holds the
  /// account's name, its type and at most the last four digits of its number. Nothing here pretends to reach the bank.
  Future<BankConnection> connectBank({
    required String accountName,
    String last4 = '',
    String accountType = 'checking',
  }) async {
    final headers = await _getHeaders();
    final response = await _client.post(
      Uri.parse('$_bankingServiceUrl/connections'),
      headers: headers,
      body: json.encode({
        'provider': 'manual',
        'account_name': accountName,
        'account_number_last4': last4,
        'account_type': accountType,
      }),
    );

    if (response.statusCode == 200 || response.statusCode == 201) {
      return BankConnection.fromJson(
        json.decode(response.body) as Map<String, dynamic>,
      );
    } else {
      throw Exception('Failed to connect bank account: ${response.body}');
    }
  }

  /// List the caller's bank connections in the current Book context.
  Future<List<BankConnection>> getConnections() async {
    final headers = await _getHeaders();
    final response = await _client.get(
      Uri.parse('$_bankingServiceUrl/connections'),
      headers: headers,
    );

    if (response.statusCode == 200) {
      final body = json.decode(response.body);
      // The engine names the count and the list together; a bare list is still read so an older shape cannot break
      // the screen.
      final rows = body is Map<String, dynamic>
          ? (body['connections'] as List<dynamic>? ?? const [])
          : (body as List<dynamic>);
      return rows
          .map((j) => BankConnection.fromJson(j as Map<String, dynamic>))
          .toList();
    } else {
      throw Exception('Could not read the bank accounts: ${response.body}');
    }
  }

  /// Ask the bank connection to sync (returns the synced-at timestamp).
  Future<void> syncTransactions(String connectionId) async {
    final headers = await _getHeaders();
    final response = await _client.post(
      Uri.parse('$_bankingServiceUrl/sync/start'),
      headers: headers,
      body: json.encode({'bank_connection_id': connectionId}),
    );

    if (response.statusCode != 200) {
      throw Exception('Failed to sync transactions: ${response.body}');
    }
  }

  /// List transactions for a connection (Book-scoped).
  Future<List<BankTransaction>> getTransactions(
    String connectionId, {
    int limit = 50,
    int offset = 0,
  }) async {
    final headers = await _getHeaders();
    final response = await _client.get(
      Uri.parse('$_bankingServiceUrl/transactions').replace(
        queryParameters: {
          'connection_id': connectionId,
          'limit': '$limit',
          'offset': '$offset',
        },
      ),
      headers: headers,
    );

    if (response.statusCode == 200) {
      final List<dynamic> jsonList =
          json.decode(response.body) as List<dynamic>;
      return jsonList
          .map((j) => BankTransaction.fromJson(j as Map<String, dynamic>))
          .toList();
    } else {
      throw Exception('Failed to load transactions: ${response.body}');
    }
  }

  /// Mark a transaction as reconciled against an accounting entry.
  Future<void> reconcileTransaction(
    String connectionId,
    String transactionId, {
    String? matchedEntryId,
    String notes = '',
  }) async {
    final headers = await _getHeaders();
    final response = await _client.post(
      Uri.parse('$_bankingServiceUrl/transactions/$transactionId/reconcile'),
      headers: headers,
      body: json.encode({
        if (matchedEntryId != null) 'matched_entry_id': matchedEntryId,
        'notes': notes,
      }),
    );

    if (response.statusCode != 200) {
      throw Exception('Failed to reconcile transaction: ${response.body}');
    }
  }

  /// Remove a bank connection.
  Future<void> disconnect(String connectionId) async {
    final headers = await _getHeaders();
    final response = await _client.delete(
      Uri.parse('$_bankingServiceUrl/connections/$connectionId'),
      headers: headers,
    );

    if (response.statusCode != 200 && response.statusCode != 204) {
      throw Exception('Failed to disconnect bank account: ${response.body}');
    }
  }
}
