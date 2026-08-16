// mobile-client/lib/services/accounting_api_service.dart

import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/models/accounting_models.dart'; // Ensure these models exist or are defined
import 'package:vimbai_mobile_client/services/auth_service.dart'; // For getting authentication token
import 'package:decimal/decimal.dart'; // Import for Decimal type

class AccountingApiService {
  final String _baseUrl = 'http://localhost:8000'; // Replace with actual Accounting Service URL
  final AuthService _authService = AuthService(); // Assuming an AuthService for token management

  Future<Map<String, String>> _getHeaders() async {
    final token = await _authService.getToken();
    return {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer $token',
    };
  }

  // --- Journal Entry Endpoints ---

  Future<JournalEntryInDB> createJournalEntry(JournalEntryCreate entry, {bool isSynced = true}) async {
    final response = await http.post(
      Uri.parse('$_baseUrl/journal-entries/'),
      headers: await _getHeaders(),
      body: json.encode(entry.toJson()), // Use a toJson method on your models
    );

    if (response.statusCode == 201) {
      return JournalEntryInDB.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to create journal entry: ${response.body}');
    }
  }

  Future<JournalEntryInDB> getJournalEntry(String entryId) async {
    final response = await http.get(
      Uri.parse('$_baseUrl/journal-entries/$entryId'),
      headers: await _getHeaders(),
    );

    if (response.statusCode == 200) {
      return JournalEntryInDB.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to get journal entry: ${response.body}');
    }
  }

  Future<List<JournalEntryInDB>> getAllJournalEntries() async {
    final response = await http.get(
      Uri.parse('$_baseUrl/journal-entries/'),
      headers: await _getHeaders(),
    );

    if (response.statusCode == 200) {
      Iterable l = json.decode(response.body);
      return List<JournalEntryInDB>.from(l.map((model) => JournalEntryInDB.fromJson(model)));
    } else {
      throw Exception('Failed to get all journal entries: ${response.body}');
    }
  }

  Future<JournalEntryInDB> updateJournalEntry(String entryId, JournalEntryUpdate entry) async {
    final response = await http.put(
      Uri.parse('$_baseUrl/journal-entries/$entryId'),
      headers: await _getHeaders(),
      body: json.encode(entry.toJson()),
    );

    if (response.statusCode == 200) {
      return JournalEntryInDB.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to update journal entry: ${response.body}');
    }
  }

  Future<void> deleteJournalEntry(String entryId) async {
    final response = await http.delete(
      Uri.parse('$_baseUrl/journal-entries/$entryId'),
      headers: await _getHeaders(),
    );

    if (response.statusCode != 204) {
      throw Exception('Failed to delete journal entry: ${response.body}');
    }
  }

  // --- Account Endpoints ---

  Future<AccountInDB> createAccount(AccountCreate account) async {
    final response = await http.post(
      Uri.parse('$_baseUrl/accounts/'),
      headers: await _getHeaders(),
      body: json.encode(account.toJson()),
    );

    if (response.statusCode == 201) {
      return AccountInDB.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to create account: ${response.body}');
    }
  }

  Future<List<AccountInDB>> getAllAccounts() async {
    final response = await http.get(
      Uri.parse('$_baseUrl/accounts/'),
      headers: await _getHeaders(),
    );

    if (response.statusCode == 200) {
      Iterable l = json.decode(response.body);
      return List<AccountInDB>.from(l.map((model) => AccountInDB.fromJson(model)));
    } else {
      throw Exception('Failed to get all accounts: ${response.body}');
    }
  }

  Future<AccountInDB> getAccount(String accountNumber) async {
    final response = await http.get(
      Uri.parse('$_baseUrl/accounts/$accountNumber'),
      headers: await _getHeaders(),
    );

    if (response.statusCode == 200) {
      return AccountInDB.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to get account: ${response.body}');
    }
  }

  Future<AccountInDB> updateAccount(String accountNumber, AccountUpdate account) async {
    final response = await http.put(
      Uri.parse('$_baseUrl/accounts/$accountNumber'),
      headers: await _getHeaders(),
      body: json.encode(account.toJson()),
    );

    if (response.statusCode == 200) {
      return AccountInDB.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to update account: ${response.body}');
    }
  }

  Future<void> deleteAccount(String accountNumber) async {
    final response = await http.delete(
      Uri.parse('$_baseUrl/accounts/$accountNumber'),
      headers: await _getHeaders(),
    );

    if (response.statusCode != 204) {
      throw Exception('Failed to delete account: ${response.body}');
    }
  }
}
