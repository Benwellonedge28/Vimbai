import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:finacc_mobile_client/local_db/user_local_data.dart';
import 'package:finacc_mobile_client/config.dart'; // For API URL

// Models needed for Chart of Accounts and Journal Entries
// (Assuming these models are defined in mobile-client/lib/models/accounting_models.dart,
// or are simple maps/dynamic types for now)
class Account {
  final String accountNumber;
  final String accountName;
  final String accountType;
  final String normalBalance;
  final String? description;
  final String? parentAccountNumber;
  final String id;
  final DateTime createdAt;
  final DateTime updatedAt;

  Account({
    required this.accountNumber,
    required this.accountName,
    required this.accountType,
    required this.normalBalance,
    this.description,
    this.parentAccountNumber,
    required this.id,
    required this.createdAt,
    required this.updatedAt,
  });

  factory Account.fromJson(Map<String, dynamic> json) {
    return Account(
      accountNumber: json['account_number'],
      accountName: json['account_name'],
      accountType: json['account_type'],
      normalBalance: json['normal_balance'],
      description: json['description'],
      parentAccountNumber: json['parent_account_number'],
      id: json['id'],
      createdAt: DateTime.parse(json['created_at']),
      updatedAt: DateTime.parse(json['updated_at']),
    );
  }
}

class JournalLine {
  final String accountNumber;
  final double debit;
  final double credit;
  final String? description;

  JournalLine({
    required this.accountNumber,
    required this.debit,
    required this.credit,
    this.description,
  });

  Map<String, dynamic> toJson() {
    return {
      'account_number': accountNumber,
      'debit': debit,
      'credit': credit,
      'description': description,
    };
  }
}

class JournalEntry {
  final DateTime entryDate;
  final String description;
  final String? referenceNumber;
  final String sourceModule;
  final List<JournalLine> lines;
  final String? id; // Optional for creation, required for fetched entry
  final DateTime? createdAt;
  final DateTime? updatedAt;

  JournalEntry({
    required this.entryDate,
    required this.description,
    this.referenceNumber,
    required this.sourceModule,
    required this.lines,
    this.id,
    this.createdAt,
    this.updatedAt,
  });

  factory JournalEntry.fromJson(Map<String, dynamic> json) {
    var linesFromJson = json['lines'] as List;
    List<JournalLine> linesList = linesFromJson.map((lineJson) => JournalLine(
      accountNumber: lineJson['account_number'],
      debit: (lineJson['debit'] as num).toDouble(),
      credit: (lineJson['credit'] as num).toDouble(),
      description: lineJson['description'],
    )).toList();

    return JournalEntry(
      entryDate: DateTime.parse(json['entry_date']),
      description: json['description'],
      referenceNumber: json['reference_number'],
      sourceModule: json['source_module'],
      lines: linesList,
      id: json['id'],
      createdAt: DateTime.parse(json['created_at']),
      updatedAt: DateTime.parse(json['updated_at']),
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'entry_date': entryDate.toIso8601String(),
      'description': description,
      'reference_number': referenceNumber,
      'source_module': sourceModule,
      'lines': lines.map((line) => line.toJson()).toList(),
    };
  }
}


class AccountingApiService {
  final String _baseUrl = AppConfig.apiUrl; // Base URL from config
  final String _accountingServiceUrl = '${AppConfig.apiUrl.replaceFirst(':8080', ':8000')}'; // Hardcoded for now, will use API Gateway path later

  Future<Map<String, String>> _getHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer $token',
    };
  }

  // --- Chart of Accounts (COA) API Calls ---
  Future<List<Account>> getChartOfAccounts() async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_accountingServiceUrl/accounts/'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> accountsJson = json.decode(response.body);
      return accountsJson.map((json) => Account.fromJson(json)).toList();
    } else {
      throw Exception('Failed to load chart of accounts: ${response.body}');
    }
  }

  // --- Journal Entry API Calls ---
  Future<JournalEntry> createJournalEntry(JournalEntry entry) async {
    final headers = await _getHeaders();
    final response = await http.post(
      Uri.parse('$_accountingServiceUrl/journal-entries/'),
      headers: headers,
      body: json.encode(entry.toJson()),
    );

    if (response.statusCode == 201) {
      return JournalEntry.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to create journal entry: ${response.body}');
    }
  }

  Future<List<JournalEntry>> getJournalEntries() async {
    final headers = await _getHeaders();
    final response = await http.get(Uri.parse('$_accountingServiceUrl/journal-entries/'), headers: headers);

    if (response.statusCode == 200) {
      List<dynamic> entriesJson = json.decode(response.body);
      return entriesJson.map((json) => JournalEntry.fromJson(json)).toList();
    } else {
      throw Exception('Failed to load journal entries: ${response.body}');
    }
  }
}
