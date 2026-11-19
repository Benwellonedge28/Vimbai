// What you owe the taxman: the tax the Book has actually posted, added over a period.
//
// The figure is not a rate applied to a total. Every taxed sale posts its tax to the Book's own VAT account and every
// taxed purchase posts its reclaim to the same account, so what was collected is the credit side and what can be
// reclaimed is the debit side, and the difference is what has to be paid. This file reads that answer and says it in a
// sentence; it does no arithmetic of its own beyond subtracting the two figures the engine gave.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One document that put tax into the account, so a total can be followed to the invoice behind it.
class TaxDocument {
  const TaxDocument({
    required this.kind,
    required this.number,
    required this.date,
    required this.total,
    required this.tax,
  });

  /// `sale` or `purchase`.
  final String kind;
  final String number;
  final String date;
  final ExactAmount total;
  final ExactAmount tax;

  factory TaxDocument.fromJson(Map<String, dynamic> json) => TaxDocument(
    kind: (json['kind'] as String?) ?? '',
    number: (json['number'] as String?) ?? '',
    date: (json['date'] as String?) ?? '',
    total: ExactAmount.fromJson(json['total']),
    tax: ExactAmount.fromJson(json['tax']),
  );

  bool get isSale => kind == 'sale';

  /// "Sale INV-1, 2026-10-05" and the tax it carried.
  String get reading => '${isSale ? 'Sale' : 'Purchase'} $number, $date';
}

/// One account the tax was posted to.
class TaxAccountHolding {
  const TaxAccountHolding({
    required this.accountNumber,
    required this.name,
    required this.collected,
    required this.reclaimable,
  });

  final String accountNumber;
  final String name;
  final ExactAmount collected;
  final ExactAmount reclaimable;

  factory TaxAccountHolding.fromJson(Map<String, dynamic> json) =>
      TaxAccountHolding(
        accountNumber: (json['account_number'] as String?) ?? '',
        name: (json['name'] as String?) ?? '',
        collected: ExactAmount.fromJson(json['collected']),
        reclaimable: ExactAmount.fromJson(json['reclaimable']),
      );

  String get label => '$accountNumber $name';
}

/// What the Book's tax account holds over a period.
class TaxDue {
  const TaxDue({
    required this.from,
    required this.to,
    required this.collected,
    required this.reclaimable,
    required this.due,
    required this.accounts,
    required this.documents,
    required this.hasTaxAccount,
  });

  final String from;
  final String to;
  final ExactAmount collected;
  final ExactAmount reclaimable;

  /// What is owed. Negative means the period reclaims more than it collected.
  final ExactAmount due;

  final List<TaxAccountHolding> accounts;
  final List<TaxDocument> documents;

  /// Whether the Book has an account whose name says VAT or tax. Without one nothing can have been posted, so a nil
  /// figure here is a missing account rather than a nil return.
  final bool hasTaxAccount;

  factory TaxDue.fromJson(Map<String, dynamic> json) => TaxDue(
    from: (json['from'] as String?) ?? '',
    to: (json['to'] as String?) ?? '',
    collected: ExactAmount.fromJson(json['collected']),
    reclaimable: ExactAmount.fromJson(json['reclaimable']),
    due: ExactAmount.fromJson(json['due']),
    accounts: [
      for (final row in (json['accounts'] as List?) ?? const [])
        TaxAccountHolding.fromJson(row as Map<String, dynamic>),
    ],
    documents: [
      for (final row in (json['documents'] as List?) ?? const [])
        TaxDocument.fromJson(row as Map<String, dynamic>),
    ],
    hasTaxAccount: json['has_tax_account'] == true,
  );

  bool get reclaiming => due.isNegative;

  /// Whether anything at all was taxed in the period.
  bool get nothingTaxed => documents.isEmpty;

  /// What to say first.
  ///
  /// The amount owed leads when there is one, because that is the money somebody has to find. A period that reclaims more
  /// than it collected says so rather than showing a negative number as though it were a debt, and a Book with no tax
  /// account says what to add instead of reporting a confident nil.
  String get headline {
    if (!hasTaxAccount) {
      return 'This Book has no VAT account';
    }
    if (nothingTaxed && due.isZero) {
      return 'No tax was charged in this period';
    }
    if (reclaiming) {
      return '${(ExactAmount.zero - due).toDecimalString()} to reclaim';
    }
    return '${due.toDecimalString()} owed in tax';
  }

  /// The sentence under the headline.
  String get detail {
    if (!hasTaxAccount) {
      return 'Tax is posted to an account whose name says VAT or tax, and this Book has none, so nothing could have '
          'been recorded. Add one in Chart of accounts.';
    }
    if (nothingTaxed && due.isZero) {
      return 'Nothing was charged or reclaimed between $from and $to.';
    }
    final parts = <String>[
      '${collected.toDecimalString()} collected on sales',
      '${reclaimable.toDecimalString()} reclaimed on purchases',
    ];
    return '${parts.join(', ')}, between $from and $to.';
  }
}

/// The door that answers the tax reading.
class TaxDueService {
  TaxDueService._();

  static final TaxDueService instance = TaxDueService._();

  /// A second instance for a test that needs a transport of its own.
  TaxDueService.fresh();

  static const String _base = AppConfig.accountingRoute;

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

  /// What the tax account holds between two days, both ends included.
  Future<TaxDue> read({required String from, required String to}) async {
    final answer = await _client.get(
      Uri.parse('$_base/tax?from=$from&to=$to'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return TaxDue.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }
}

/// The first and last day of the month a day falls in, which is the period a tax return covers.
///
/// A month rather than a quarter because that is the period the account is kept by, and the last day is worked out from
/// the month rather than assumed to be the thirtieth.
({String from, String to}) monthWindow(String today) {
  final trimmed = today.trim();
  if (trimmed.length < 7) return (from: '', to: '');
  final year = int.tryParse(trimmed.substring(0, 4));
  final month = int.tryParse(trimmed.substring(5, 7));
  if (year == null || month == null || month < 1 || month > 12) {
    return (from: '', to: '');
  }
  // the day before the first of the next month is the last of this one, whatever its length
  final firstOfNext = month == 12
      ? DateTime(year + 1, 1, 1)
      : DateTime(year, month + 1, 1);
  final last = firstOfNext.subtract(const Duration(days: 1));
  final monthText = month.toString().padLeft(2, '0');
  final lastText = last.day.toString().padLeft(2, '0');
  return (from: '$year-$monthText-01', to: '$year-$monthText-$lastText');
}
