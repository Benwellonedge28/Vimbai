// mobile-client/lib/models/accounting_models.dart

import 'package:decimal/decimal.dart';
import 'package:json_annotation/json_annotation.dart';

part 'accounting_models.g.dart'; // Generated file for json_serializable

// --- Journal Entry Models ---
@JsonSerializable()
class JournalLineCreate {
  final String accountNumber;
  final Decimal debit;
  final Decimal credit;
  final String? description;

  JournalLineCreate({
    required this.accountNumber,
    required this.debit,
    required this.credit,
    this.description,
  });

  factory JournalLineCreate.fromJson(Map<String, dynamic> json) => _$JournalLineCreateFromJson(json);
  Map<String, dynamic> toJson() => _$JournalLineCreateToJson(this);
}

@JsonSerializable()
class JournalEntryCreate {
  final DateTime entryDate;
  final String description;
  final String sourceModule;
  final String? referenceNumber;
  final List<JournalLineCreate> lines;

  JournalEntryCreate({
    required this.entryDate,
    required this.description,
    required this.sourceModule,
    this.referenceNumber,
    required this.lines,
  });

  factory JournalEntryCreate.fromJson(Map<String, dynamic> json) => _$JournalEntryCreateFromJson(json);
  Map<String, dynamic> toJson() => _$JournalEntryCreateToJson(this);
}

@JsonSerializable()
class JournalEntryUpdate {
  final DateTime? entryDate;
  final String? description;
  final String? sourceModule;
  final String? referenceNumber;
  final List<JournalLineCreate>? lines;

  JournalEntryUpdate({
    this.entryDate,
    this.description,
    this.sourceModule,
    this.referenceNumber,
    this.lines,
  });

  factory JournalEntryUpdate.fromJson(Map<String, dynamic> json) => _$JournalEntryUpdateFromJson(json);
  Map<String, dynamic> toJson() => _$JournalEntryUpdateToJson(this);
}

@JsonSerializable()
class JournalEntryInDB {
  final String id;
  final DateTime entryDate;
  final String description;
  final String sourceModule;
  final String? referenceNumber;
  final List<JournalLineInDB> lines; // Note: using JournalLineInDB for fetched data
  final DateTime createdAt;
  final DateTime updatedAt;

  JournalEntryInDB({
    required this.id,
    required this.entryDate,
    required this.description,
    required this.sourceModule,
    this.referenceNumber,
    required this.lines,
    required this.createdAt,
    required this.updatedAt,
  });

  factory JournalEntryInDB.fromJson(Map<String, dynamic> json) => _$JournalEntryInDBFromJson(json);
  Map<String, dynamic> toJson() => _$JournalEntryInDBToJson(this);
}

// Separate model for lines in DB, as it might have its own ID or timestamp
@JsonSerializable()
class JournalLineInDB {
  final String id;
  final String accountNumber;
  final Decimal debit;
  final Decimal credit;
  final String? description;

  JournalLineInDB({
    required this.id,
    required this.accountNumber,
    required this.debit,
    required this.credit,
    this.description,
  });

  factory JournalLineInDB.fromJson(Map<String, dynamic> json) => _$JournalLineInDBFromJson(json);
  Map<String, dynamic> toJson() => _$JournalLineInDBToJson(this);
}


// --- Account Models ---
@JsonSerializable()
class AccountCreate {
  final String accountNumber;
  final String accountName;
  final String accountType;
  final Decimal currentBalance;
  final String? description;

  AccountCreate({
    required this.accountNumber,
    required this.accountName,
    required this.accountType,
    required this.currentBalance,
    this.description,
  });

  factory AccountCreate.fromJson(Map<String, dynamic> json) => _$AccountCreateFromJson(json);
  Map<String, dynamic> toJson() => _$AccountCreateToJson(this);
}

@JsonSerializable()
class AccountUpdate {
  final String? accountName;
  final String? accountType;
  final Decimal? currentBalance;
  final String? description;

  AccountUpdate({
    this.accountName,
    this.accountType,
    this.currentBalance,
    this.description,
  });

  factory AccountUpdate.fromJson(Map<String, dynamic> json) => _$AccountUpdateFromJson(json);
  Map<String, dynamic> toJson() => _$AccountUpdateToJson(this);
}

@JsonSerializable()
class AccountInDB {
  final String id;
  final String accountNumber;
  final String accountName;
  final String accountType;
  final Decimal currentBalance;
  final String? description;
  final DateTime createdAt;
  final DateTime updatedAt;

  AccountInDB({
    required this.id,
    required this.accountNumber,
    required this.accountName,
    required this.accountType,
    required this.currentBalance,
    this.description,
    required this.createdAt,
    required this.updatedAt,
  });

  factory AccountInDB.fromJson(Map<String, dynamic> json) => _$AccountInDBFromJson(json);
  Map<String, dynamic> toJson() => _$AccountInDBToJson(this);
}

// --- Placeholder for Auth Service ---
// This would be a real service for handling tokens, login, etc.
class AuthService {
  Future<String> getToken() async {
    // Implement actual token retrieval (e.g., from secure storage)
    return 'mock_auth_token';
  }
}
