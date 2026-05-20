import 'package:flutter/foundation.dart';
import 'package:decimal/decimal.dart'; // Assuming decimal package for precision
import 'package:intl/intl.dart'; // For date formatting
import 'package:flutter/material.dart';

// To use Decimal, you'd need to add it to pubspec.yaml:
// decimal: ^2.3.3
// For now, I'll use double for simplicity, but note the precision issue
// and replace with Decimal as appropriate in a real app.

class BudgetItem {
  final String? id;
  final String category;
  final double budgetedAmount;
  final double actualAmount;
  final String? description;
  final String? accountNumber;
  final DateTime periodStart;
  final DateTime periodEnd;
  final DateTime? createdAt;
  final DateTime? updatedAt;

  BudgetItem({
    this.id,
    required this.category,
    required this.budgetedAmount,
    this.actualAmount = 0.0,
    this.description,
    this.accountNumber,
    required this.periodStart,
    required this.periodEnd,
    this.createdAt,
    this.updatedAt,
  });

  factory BudgetItem.fromJson(Map<String, dynamic> json) {
    return BudgetItem(
      id: json['id'],
      category: json['category'],
      budgetedAmount: (json['budgeted_amount'] as num).toDouble(),
      actualAmount: (json['actual_amount'] as num).toDouble(),
      description: json['description'],
      accountNumber: json['account_number'],
      periodStart: DateTime.parse(json['period_start']),
      periodEnd: DateTime.parse(json['period_end']),
      createdAt: json['created_at'] != null ? DateTime.parse(json['created_at']) : null,
      updatedAt: json['updated_at'] != null ? DateTime.parse(json['updated_at']) : null,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'category': category,
      'budgeted_amount': budgetedAmount,
      'actual_amount': actualAmount,
      'description': description,
      'account_number': accountNumber,
      'period_start': periodStart.toIso8601String(),
      'period_end': periodEnd.toIso8601String(),
    };
  }
}

class Budget {
  final String? id;
  final String name;
  final int fiscalYear;
  final String period;
  final String? description;
  final String status;
  final List<BudgetItem> items;
  final DateTime? createdAt;
  final DateTime? updatedAt;

  Budget({
    this.id,
    required this.name,
    required this.fiscalYear,
    required this.period,
    this.description,
    required this.status,
    required this.items,
    this.createdAt,
    this.updatedAt,
  });

  factory Budget.fromJson(Map<String, dynamic> json) {
    var itemsFromJson = json['items'] as List;
    List<BudgetItem> itemsList = itemsFromJson.map((itemJson) => BudgetItem.fromJson(itemJson)).toList();

    return Budget(
      id: json['id'],
      name: json['name'],
      fiscalYear: json['fiscal_year'],
      period: json['period'],
      description: json['description'],
      status: json['status'],
      items: itemsList,
      createdAt: json['created_at'] != null ? DateTime.parse(json['created_at']) : null,
      updatedAt: json['updated_at'] != null ? DateTime.parse(json['updated_at']) : null,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'name': name,
      'fiscal_year': fiscalYear,
      'period': period,
      'description': description,
      'status': status,
      'items': items.map((item) => item.toJson()).toList(),
    };
  }
}
