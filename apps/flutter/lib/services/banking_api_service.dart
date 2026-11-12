import 'dart:convert';
import 'package:vimbai_mobile_client/services/api_client.dart';
import 'package:vimbai_mobile_client/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/config.dart';
import 'package:vimbai_mobile_client/models/banking_models.dart';
import 'package:vimbai_mobile_client/services/book_context.dart';

/// Client for the banking-integration-service via the API gateway.
///
/// Real contract (all Book-scoped by the gateway's X-Book-ID injection):
///   POST   /connect                        query: bank_name, account_number,
///                                         api_key, account_type
///   GET    /connections
///   POST   /sync/{connection_id}
///   GET    /transactions/{connection_id}   query: limit, offset
///   POST   /transactions/{connection_id}/reconcile
///          body: {transaction_id, matched_entry_id?, notes?}
///   DELETE /connections/{connection_id}
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
  Future<BankConnection> connectBank({
    required String bankName,
    required String accountNumber,
    required String apiKey,
    String accountType = 'checking',
  }) async {
    final headers = await _getHeaders();
    final response = await _client.post(
      Uri.parse('$_bankingServiceUrl/connect').replace(queryParameters: {
        'bank_name': bankName,
        'account_number': accountNumber,
        'api_key': apiKey,
        'account_type': accountType,
      }),
      headers: headers,
    );

    if (response.statusCode == 200 || response.statusCode == 201) {
      return BankConnection.fromJson(json.decode(response.body) as Map<String, dynamic>);
    } else {
      throw Exception('Failed to connect bank account: ${response.body}');
    }
  }

  /// List the caller's bank connections in the current Book context.
  Future<List<BankConnection>> getConnections() async {
    final headers = await _getHeaders();
    final response = await _client.get(Uri.parse('$_bankingServiceUrl/connections'), headers: headers);

    if (response.statusCode == 200) {
      final List<dynamic> jsonList = json.decode(response.body) as List<dynamic>;
      return jsonList.map((j) => BankConnection.fromJson(j as Map<String, dynamic>)).toList();
    } else {
      throw Exception('Failed to load bank connections: ${response.body}');
    }
  }

  /// Ask the bank connection to sync (returns the synced-at timestamp).
  Future<void> syncTransactions(String connectionId) async {
    final headers = await _getHeaders();
    final response = await _client.post(
      Uri.parse('$_bankingServiceUrl/sync/$connectionId'),
      headers: headers,
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
      Uri.parse('$_bankingServiceUrl/transactions/$connectionId').replace(queryParameters: {
        'limit': '$limit',
        'offset': '$offset',
      }),
      headers: headers,
    );

    if (response.statusCode == 200) {
      final List<dynamic> jsonList = json.decode(response.body) as List<dynamic>;
      return jsonList.map((j) => BankTransaction.fromJson(j as Map<String, dynamic>)).toList();
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
      Uri.parse('$_bankingServiceUrl/transactions/$connectionId/reconcile'),
      headers: headers,
      body: json.encode({
        'transaction_id': transactionId,
        'matched_entry_id': matchedEntryId,
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
