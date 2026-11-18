// When the money owed to suppliers has to leave the account.
//
// A total owed says nothing about urgency: 4,000.00 owed is comfortable if none of it is due for a month and alarming if
// 2,000.00 of it was due last week. The engine places each supplier's open bills by their own due date, and this file only
// reads that answer.
//
// Two things are deliberately not done here. Nothing is totalled from the suppliers: the engine's four figures are the
// answer, so a supplier arriving with no due date cannot silently shift a total. And a bill with no due date is never
// treated as due today - that would invent a deadline the supplier never gave, and the first supplier a person pays would
// be whichever one forgot to write a date down.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// What is owed to one supplier and how soon it falls due.
class SupplierDue {
  final String supplierId;
  final String name;
  final ExactAmount owed;

  /// The part of [owed] whose bill has already passed its due date.
  final ExactAmount overdue;

  /// The earliest day money is due to them, empty when no open bill carries a date.
  final String dueDate;
  final int openBills;

  const SupplierDue({
    required this.supplierId,
    required this.name,
    required this.owed,
    required this.overdue,
    required this.dueDate,
    required this.openBills,
  });

  factory SupplierDue.fromJson(Map<String, dynamic> json) => SupplierDue(
    supplierId: (json['creditor_id'] as String?) ?? '',
    name: (json['creditor_name'] as String?) ?? '',
    owed: ExactAmount.fromJson(json['owed']),
    overdue: ExactAmount.fromJson(json['overdue']),
    dueDate: (json['due_date'] as String?) ?? '',
    openBills: (json['open_bills'] as num?)?.toInt() ?? 0,
  );

  bool get isLate => overdue > ExactAmount.zero;

  /// How the urgency reads on one line.
  String get timing {
    if (isLate) {
      return '${overdue.toDecimalString()} was due by $dueDate';
    }
    if (dueDate.isEmpty) return 'No due date given';
    return 'Due $dueDate';
  }
}

/// The Book's payables arranged by when they fall due.
class DuePlan {
  final String asOf;
  final ExactAmount overdue;
  final ExactAmount dueSoon;
  final ExactAmount later;
  final ExactAmount undated;
  final int overdueSuppliers;
  final String nextDueDate;
  final List<SupplierDue> suppliers;

  const DuePlan({
    required this.asOf,
    required this.overdue,
    required this.dueSoon,
    required this.later,
    required this.undated,
    required this.overdueSuppliers,
    required this.nextDueDate,
    required this.suppliers,
  });

  factory DuePlan.fromJson(Map<String, dynamic> json) => DuePlan(
    asOf: (json['as_of'] as String?) ?? '',
    overdue: ExactAmount.fromJson(json['overdue']),
    dueSoon: ExactAmount.fromJson(json['due_soon']),
    later: ExactAmount.fromJson(json['later']),
    undated: ExactAmount.fromJson(json['undated']),
    overdueSuppliers: (json['overdue_suppliers'] as num?)?.toInt() ?? 0,
    nextDueDate: (json['next_due_date'] as String?) ?? '',
    suppliers: [
      for (final row in (json['creditors'] as List?) ?? const [])
        SupplierDue.fromJson(row as Map<String, dynamic>),
    ],
  );

  bool get nothingOwed => suppliers.isEmpty;

  ExactAmount get total => overdue + dueSoon + later + undated;

  /// What to say first.
  ///
  /// Late money leads, because that is the only part of the figure that is already a problem: a supplier who is waiting is
  /// the one who stops delivering. With nothing late the next date is what a person plans around, and with nothing dated
  /// at all the plan says so rather than implying everything is due today.
  String get headline {
    if (nothingOwed) return 'You owe nothing to suppliers';
    if (overdue > ExactAmount.zero) {
      final who = overdueSuppliers == 1
          ? '1 supplier'
          : '$overdueSuppliers suppliers';
      return '${overdue.toDecimalString()} is late to $who';
    }
    if (nextDueDate.isNotEmpty) {
      return 'Nothing is late. Next ${nextDueDate}';
    }
    return 'Nothing is late, and no open bill has a due date';
  }

  /// The sentence under the headline, or empty when there is nothing to add.
  String get detail {
    if (nothingOwed) return '';
    final parts = <String>[];
    if (dueSoon > ExactAmount.zero) {
      parts.add('${dueSoon.toDecimalString()} within a week');
    }
    if (later > ExactAmount.zero) {
      parts.add('${later.toDecimalString()} later');
    }
    if (undated > ExactAmount.zero) {
      parts.add('${undated.toDecimalString()} with no date on the bill');
    }
    if (parts.isEmpty) return '${total.toDecimalString()} owed in total';
    return '${total.toDecimalString()} owed in total: ${parts.join(', ')}';
  }
}

/// The door that answers the plan.
class DuePlanService {
  DuePlanService._();

  static final DuePlanService instance = DuePlanService._();

  /// A second instance for a test that needs a transport of its own.
  DuePlanService.fresh();

  static const String _base = AppConfig.apiUrl;
  static const String _door = '/purchases-ledger-control';

  http.Client _client = http.Client();

  @visibleForTesting
  void useClient(http.Client client) => _client = client;

  String? _token;

  @visibleForTesting
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// What falls due when, as at [asOf] (a plain day, so the answer does not drift through the day).
  Future<DuePlan> plan(String asOf) async {
    final answer = await _client.get(
      Uri.parse('$_base$_door/due?as_of_date=$asOf'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return DuePlan.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }
}
