// The discounts this Book gives, as the Book itself declares them.
//
// A discount is a row the owner can add: a name, a percentage or a sum of money, whether it is given on every invoice it
// fits or only when somebody takes it, who it is for, the figure an invoice must reach before it applies, and the days
// it holds for. Nothing about a discount is built into this app, so adding one is a row rather than a release.
//
// The app never works out a discount itself. It asks the engine what would come off an invoice, and shows that answer,
// so the figure on the phone and the figure in the journal can never disagree.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One discount the Book gives.
class BookDiscount {
  const BookDiscount({
    required this.id,
    required this.name,
    required this.kind,
    required this.value,
    required this.compulsory,
    required this.allCustomers,
    required this.customerIds,
    required this.minTotal,
    required this.effectiveFrom,
    required this.effectiveTo,
  });

  final String id;
  final String name;

  /// `percent` or `amount`: how the value reads.
  final String kind;

  /// The percentage itself or the sum of money, exactly as the Book holds it.
  final ExactAmount value;

  /// Given on every invoice it fits, without anybody asking.
  final bool compulsory;

  /// For every customer, or only the ones named.
  final bool allCustomers;

  final List<String> customerIds;

  /// The figure an invoice must reach before the discount applies. Zero means every invoice.
  final ExactAmount minTotal;

  final String effectiveFrom;
  final String effectiveTo;

  factory BookDiscount.fromJson(Map<String, dynamic> json) => BookDiscount(
    id: (json['id'] as String?) ?? '',
    name: (json['name'] as String?) ?? '',
    kind: (json['kind'] as String?) == 'percent' ? 'percent' : 'amount',
    value: ExactAmount.fromJson(json['value']),
    compulsory: json['compulsory'] == true,
    allCustomers: json['all_customers'] != false,
    customerIds: [
      for (final row in (json['customer_ids'] as List?) ?? const [])
        if (row is String && row.isNotEmpty) row,
    ],
    minTotal: ExactAmount.fromJson(json['min_total']),
    effectiveFrom: (json['effective_from'] as String?) ?? '',
    effectiveTo: (json['effective_to'] as String?) ?? '',
  );

  bool get byPercent => kind == 'percent';

  /// "10%" or "250.00", without the trailing zeros the engine uses to keep the figure exact.
  String get valueText {
    final text = _trim(value.toDecimalString());
    return byPercent ? '$text%' : text;
  }

  /// "off every invoice" or "off invoices of 5,000.00 and more".
  String get thresholdWord => minTotal.isZero
      ? 'every invoice'
      : 'invoices of ${_trim(minTotal.toDecimalString())} and more';

  /// "for every customer" or "for ACME-1 and OTHER-2".
  String get whoWord =>
      allCustomers ? 'for every customer' : 'for ${customerIds.join(' and ')}';

  /// "given automatically" or "given only when it is taken".
  String get howWord =>
      compulsory ? 'given automatically' : 'taken only when it is asked for';

  /// "2026-01-01 to 2026-12-31": every discount runs from a day to a day.
  String get datesWord => effectiveTo.isEmpty
      ? 'from $effectiveFrom'
      : '$effectiveFrom to $effectiveTo';

  /// The row in one sentence: what it takes off, off what, for whom, and when it holds.
  String get reading =>
      '$name takes $valueText off $thresholdWord $whoWord, $howWord, $datesWord.';

  /// What is wrong with this discount, in the words the fix would use, or nothing when it is usable.
  String get problem {
    if (effectiveFrom.isEmpty) {
      return '$name has no start date, so it never applies. Give it a day.';
    }
    if (effectiveTo.isEmpty) {
      return '$name has no end date, so it would take money off invoices nobody meant it to reach. '
          'Say the day it stops.';
    }
    if (!allCustomers && customerIds.isEmpty) {
      return '$name is not for every customer and names nobody, so it can never be given.';
    }
    return '';
  }

  static String _trim(String text) {
    if (!text.contains('.')) return text;
    var out = text.replaceFirst(RegExp(r'0+$'), '');
    out = out.replaceFirst(RegExp(r'\.$'), '');
    return out;
  }
}

/// The discounts a Book gives.
class DiscountTable {
  const DiscountTable({required this.discounts});

  final List<BookDiscount> discounts;

  factory DiscountTable.fromJson(Map<String, dynamic> json) {
    final rows = json['discounts'] is List
        ? json['discounts'] as List
        : (json['discount_rules'] as List?) ?? const [];
    return DiscountTable(
      discounts: [
        for (final row in rows)
          if (row is Map<String, dynamic>) BookDiscount.fromJson(row),
      ],
    );
  }

  bool get empty => discounts.isEmpty;

  List<BookDiscount> get automatic => [
    for (final one in discounts)
      if (one.compulsory) one,
  ];

  List<BookDiscount> get offered => [
    for (final one in discounts)
      if (!one.compulsory) one,
  ];

  /// What to say first: whether anything comes off at all, and how many rules there are.
  String get headline {
    if (empty) return 'This Book gives no discount yet';
    final count = discounts.length;
    final auto = automatic.length;
    if (count == 1) {
      return auto == 1
          ? 'One discount, given automatically'
          : 'One discount, taken only when it is asked for';
    }
    return auto == 0
        ? '$count discounts, all taken only when they are asked for'
        : '$count discounts, $auto of them given automatically';
  }

  /// The sentence under the headline, naming what comes off and for whom.
  String get detail {
    if (empty) {
      return 'No invoice carries a discount until one is added here. Add one, and every invoice it fits takes it '
          'off by itself.';
    }
    final parts = <String>[
      for (final one in discounts)
        '${one.name} ${one.valueText} ${one.whoWord}',
    ];
    return '${parts.join('; ')}. A discount comes off before tax, so the customer is taxed on what they pay.';
  }
}

/// One discount the engine would take off an invoice.
class DiscountLine {
  const DiscountLine({
    required this.name,
    required this.kind,
    required this.value,
    required this.amount,
  });

  final String name;
  final String kind;
  final ExactAmount value;
  final ExactAmount amount;

  factory DiscountLine.fromJson(Map<String, dynamic> json) => DiscountLine(
    name: (json['name'] as String?) ?? '',
    kind: (json['kind'] as String?) == 'percent' ? 'percent' : 'amount',
    value: ExactAmount.fromJson(json['value']),
    amount: ExactAmount.fromJson(json['amount']),
  );
}

/// What the engine says would come off an invoice, before one is raised.
class DiscountQuote {
  const DiscountQuote({
    required this.total,
    required this.discount,
    required this.after,
    required this.lines,
  });

  /// The invoice figure the quote was asked for.
  final ExactAmount total;
  final ExactAmount discount;

  /// What would be left to pay.
  final ExactAmount after;
  final List<DiscountLine> lines;

  factory DiscountQuote.fromJson(Map<String, dynamic> json) => DiscountQuote(
    total: ExactAmount.fromJson(json['total']),
    discount: ExactAmount.fromJson(json['discount']),
    after: ExactAmount.fromJson(json['after']),
    lines: [
      for (final row in (json['lines'] as List?) ?? const [])
        if (row is Map<String, dynamic>) DiscountLine.fromJson(row),
    ],
  );

  bool get takesNothing => lines.isEmpty || discount.isZero;

  /// "1,000.00 less 100.00: Trade terms 100.00. The customer pays 900.00."
  String get reading {
    if (takesNothing) {
      return 'No discount comes off ${total.toDecimalString()}, so the customer pays that.';
    }
    final parts = <String>[
      for (final line in lines) '${line.name} ${line.amount.toDecimalString()}',
    ];
    return '${total.toDecimalString()} less ${discount.toDecimalString()}: '
        '${parts.join(', ')}. The customer pays ${after.toDecimalString()}.';
  }
}

/// The door that answers the discounts a Book gives.
class DiscountService {
  DiscountService._();

  static final DiscountService instance = DiscountService._();

  /// A second instance for a test that needs a transport of its own.
  DiscountService.fresh();

  static const String _base = AppConfig.apiUrl;

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

  /// Every discount the Book gives, retired ones included so their history is readable.
  Future<DiscountTable> read() async {
    final answer = await _client.get(
      Uri.parse('$_base/invoices/discounts'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(_reason(answer.body, answer.statusCode));
    }
    return DiscountTable.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// Declare a discount, or change one. The engine decides, and says why not when it refuses.
  Future<BookDiscount> save({
    required String name,
    required String kind,
    required String value,
    required bool compulsory,
    required bool allCustomers,
    List<String> customerIds = const [],
    String minTotal = '0',
    required String effectiveFrom,
    String effectiveTo = '',
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/invoices/discounts'),
      headers: _headers,
      body: jsonEncode({
        'name': name,
        'kind': kind,
        'value': value,
        'compulsory': compulsory,
        'all_customers': allCustomers,
        'customer_ids': customerIds,
        'min_total': minTotal,
        'effective_from': effectiveFrom,
        'effective_to': effectiveTo,
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      // the engine's own sentence is the useful one: it names the customer or the day to fix
      throw Exception(_reason(answer.body, answer.statusCode));
    }
    final body = jsonDecode(answer.body) as Map<String, dynamic>;
    final saved = body['discount'] ?? body['discount_rule'] ?? body;
    if (saved is Map<String, dynamic>) return BookDiscount.fromJson(saved);
    return BookDiscount.fromJson(const {});
  }

  /// What would come off an invoice of this figure, in the order the Book declares, before anything is raised.
  Future<DiscountQuote> quote({
    required String total,
    required String customerId,
    required String date,
    List<String> rules = const [],
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/invoices/discounts/quote'),
      headers: _headers,
      body: jsonEncode({
        'total': total,
        'customer_id': customerId,
        'date': date,
        'discount_rules': rules,
      }),
    );
    if (answer.statusCode != 200) {
      throw Exception(_reason(answer.body, answer.statusCode));
    }
    return DiscountQuote.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// The engine's refusal in the engine's words, without the transport noise around it.
  String _reason(String body, int status) {
    try {
      final decoded = jsonDecode(body);
      if (decoded is Map<String, dynamic>) {
        for (final key in ['error', 'detail', 'message']) {
          final value = decoded[key];
          if (value is String && value.isNotEmpty) return value;
        }
      }
    } catch (_) {
      // a body that is not JSON is reported as it came
    }
    return 'The engine answered $status: $body';
  }
}
