// The taxes this Book charges, as the Book itself defines them.
//
// A tax is a row the owner can add: a name, a percentage, the account it posts to, whether it applies to sales,
// purchases or both, the days it holds for, and which taxes it applies after. Nothing about a tax type is built into
// this app, so adding one is a row rather than a release.
//
// The app never works out tax itself. It asks the engine what a document would carry, and shows that answer, so the
// figure on the phone and the figure in the journal can never disagree.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One tax the Book charges.
class BookTax {
  const BookTax({
    required this.id,
    required this.name,
    required this.rate,
    required this.scope,
    required this.collectedAccount,
    required this.reclaimAccount,
    required this.appliesAfter,
    required this.effectiveFrom,
    required this.effectiveTo,
    required this.inForce,
  });

  final String id;
  final String name;

  /// The percentage itself, exactly as the Book holds it, so 15.5 stays 15.5.
  final ExactAmount rate;

  /// `sales`, `purchases` or `both`.
  final String scope;

  /// The account the tax is credited to on a sale. Empty means the tax cannot be charged yet.
  final String collectedAccount;

  /// The account the tax is debited to on a purchase. Empty falls back to the collected account.
  final String reclaimAccount;

  /// The taxes this one is charged after, in order. A levy declared after VAT is charged on the VAT-inclusive figure.
  final List<String> appliesAfter;

  final String effectiveFrom;
  final String effectiveTo;

  /// Whether the tax applies on the day the table was read for.
  final bool inForce;

  factory BookTax.fromJson(Map<String, dynamic> json) => BookTax(
    id: (json['id'] as String?) ?? '',
    name: (json['name'] as String?) ?? '',
    rate: ExactAmount.fromJson(json['rate']),
    scope: (json['scope'] as String?) ?? 'both',
    collectedAccount: (json['collected_account'] as String?) ?? '',
    reclaimAccount: (json['reclaim_account'] as String?) ?? '',
    appliesAfter: [
      for (final row in (json['applies_after'] as List?) ?? const [])
        if (row is String && row.isNotEmpty) row,
    ],
    effectiveFrom: (json['effective_from'] as String?) ?? '',
    effectiveTo: (json['effective_to'] as String?) ?? '',
    inForce: json['in_force'] != false,
  );

  bool get onSales => scope == 'sales' || scope == 'both';

  bool get onPurchases => scope == 'purchases' || scope == 'both';

  /// Whether the tax can be charged at all: a rate with nowhere to post is not a tax yet.
  bool get hasAccount =>
      collectedAccount.isNotEmpty || reclaimAccount.isNotEmpty;

  /// "15.5%", without the trailing zeros the engine uses to keep the figure exact.
  String get rateText {
    var text = rate.toDecimalString();
    if (text.contains('.')) {
      text = text.replaceFirst(RegExp(r'0+$'), '');
      text = text.replaceFirst(RegExp(r'\.$'), '');
    }
    return '$text%';
  }

  /// "sales", "purchases" or "sales and purchases".
  String get scopeWord {
    if (onSales && onPurchases) return 'sales and purchases';
    if (onSales) return 'sales';
    return 'purchases';
  }

  /// "from 2026-01-01", or "2026-01-01 to 2026-06-30" once it has an end.
  String get datesWord => effectiveTo.isEmpty
      ? 'from $effectiveFrom'
      : '$effectiveFrom to $effectiveTo';

  /// "after VAT, then Levy", or nothing when it stands on its own.
  String get orderWord =>
      appliesAfter.isEmpty ? '' : 'after ${appliesAfter.join(', then ')}';

  /// The row in one sentence: what it is, what it takes, where it goes, and when it holds.
  String get reading {
    final where = collectedAccount.isEmpty
        ? 'no account yet'
        : 'account $collectedAccount';
    final order = orderWord.isEmpty ? '' : ', ${orderWord}';
    return '$name charges $rateText on $scopeWord $datesWord to $where$order.';
  }

  /// What is wrong with this tax, in the words the fix would use, or nothing when it is usable.
  String get problem {
    if (!hasAccount) {
      return '$name has no account, so it cannot be charged. Say which account it posts to.';
    }
    if (effectiveFrom.isEmpty) {
      return '$name has no start date, so it never applies. Give it a day.';
    }
    return '';
  }
}

/// The Book's tax table on one day.
class TaxTable {
  const TaxTable({required this.date, required this.taxes});

  /// The day the table was read for. A tax outside its dates is listed but not in force.
  final String date;
  final List<BookTax> taxes;

  factory TaxTable.fromJson(Map<String, dynamic> json) => TaxTable(
    date: (json['date'] as String?) ?? '',
    taxes: [
      for (final row in (json['taxes'] as List?) ?? const [])
        BookTax.fromJson(row as Map<String, dynamic>),
    ],
  );

  bool get empty => taxes.isEmpty;

  List<BookTax> get inForce => [
    for (final tax in taxes)
      if (tax.inForce) tax,
  ];

  List<BookTax> get notInForce => [
    for (final tax in taxes)
      if (!tax.inForce) tax,
  ];

  /// What to say first: whether anything is charged at all, and how many taxes there are.
  String get headline {
    if (empty) return 'This Book charges no tax yet';
    final count = inForce.length;
    if (count == 0) return 'No tax applies on $date';
    return count == 1
        ? 'One tax applies on $date'
        : '$count taxes apply on $date';
  }

  /// The sentence under the headline, naming what is charged and what is not.
  String get detail {
    if (empty) {
      return 'A document carries no tax until you add one here. Add a tax, or fill in the usual ones, and every '
          'sale and purchase can then carry its own.';
    }
    final parts = <String>[
      for (final tax in inForce) '${tax.name} ${tax.rateText}',
    ];
    final later = notInForce.length;
    final tail = later == 0
        ? ''
        : later == 1
        ? ' One more is defined but does not apply on this day.'
        : ' $later more are defined but do not apply on this day.';
    if (parts.isEmpty) return 'Nothing is in force on this day.$tail';
    return '${parts.join(', ')}.$tail';
  }
}

/// One line of tax the engine would post on a document.
class TaxLine {
  const TaxLine({
    required this.tax,
    required this.rate,
    required this.base,
    required this.amount,
    required this.side,
    required this.accountNumber,
    this.expenseAccount = '',
  });

  final String tax;
  final ExactAmount rate;
  final ExactAmount base;
  final ExactAmount amount;

  /// `credit` on a sale, `debit` on a purchase.
  final String side;
  final String accountNumber;

  /// Where a sale charges this tax as an expense, so the sale keeps the figure the customer paid. Empty on a purchase,
  /// which reclaims its tax instead of costing it.
  final String expenseAccount;

  factory TaxLine.fromJson(Map<String, dynamic> json) => TaxLine(
    tax: (json['tax'] as String?) ?? '',
    rate: ExactAmount.fromJson(json['rate']),
    base: ExactAmount.fromJson(json['base']),
    amount: ExactAmount.fromJson(json['amount']),
    side: (json['side'] as String?) ?? '',
    accountNumber: (json['account_number'] as String?) ?? '',
    expenseAccount: (json['expense_account'] as String?) ?? '',
  );

  bool get credited => side == 'credit';
}

/// What the engine says a document would carry, before anything is posted.
class TaxPreview {
  const TaxPreview({
    required this.net,
    required this.tax,
    required this.gross,
    required this.lines,
    required this.note,
    this.direction = 'sale',
  });

  final ExactAmount net;
  final ExactAmount tax;
  final ExactAmount gross;
  final List<TaxLine> lines;

  /// `sale` or `purchase`: a sale charges its tax as an expense, a purchase reclaims it.
  final String direction;

  /// The engine's own sentence, used when nothing was chosen: a document carries no tax until one is.
  final String note;

  factory TaxPreview.fromJson(Map<String, dynamic> json) => TaxPreview(
    net: ExactAmount.fromJson(json['net']),
    tax: ExactAmount.fromJson(json['tax']),
    gross: ExactAmount.fromJson(json['gross']),
    lines: [
      for (final row in (json['tax_lines'] as List?) ?? const [])
        TaxLine.fromJson(row as Map<String, dynamic>),
    ],
    note: (json['note'] as String?) ?? '',
    direction: (json['direction'] as String?) ?? 'sale',
  );

  bool get carriesNothing => lines.isEmpty || tax.isZero;

  /// "A 1000.00 sale carries 155.00: VAT 155.00 to account 2100."
  String get reading {
    if (carriesNothing) return note;
    final parts = <String>[
      for (final line in lines)
        if (!line.amount.isZero)
          '${line.tax} ${line.amount.toDecimalString()} to account '
              '${line.accountNumber}',
    ];
    if (parts.isEmpty) return note;
    if (direction == 'purchase') {
      return 'A ${gross.toDecimalString()} document reclaims '
          '${tax.toDecimalString()} of tax: ${parts.join(', ')}.';
    }
    return 'The sale records ${gross.toDecimalString()} and the tax is '
        '${tax.toDecimalString()} in expenses: ${parts.join(', ')}.';
  }
}

/// The door that answers the Book's tax table.
class TaxTableService {
  TaxTableService._();

  static final TaxTableService instance = TaxTableService._();

  /// A second instance for a test that needs a transport of its own.
  TaxTableService.fresh();

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

  /// Every tax the Book has defined, and which of them hold on a day.
  Future<TaxTable> read({String date = ''}) async {
    final answer = await _client.get(
      Uri.parse('$_base/taxes?date=$date'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(_reason(answer.body, answer.statusCode));
    }
    return TaxTable.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }

  /// Add a tax, or change one that has not been used yet. The engine decides, and says why not when it refuses.
  Future<BookTax> save({
    required String name,
    required String rate,
    required String scope,
    required String collectedAccount,
    String reclaimAccount = '',
    required String effectiveFrom,
    String effectiveTo = '',
    List<String> appliesAfter = const [],
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/taxes'),
      headers: _headers,
      body: jsonEncode({
        'name': name,
        'rate': rate,
        'scope': scope,
        'collected_account': collectedAccount,
        'reclaim_account': reclaimAccount,
        'effective_from': effectiveFrom,
        'effective_to': effectiveTo,
        'applies_after': appliesAfter,
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      // the engine's own sentence is the useful one: it names the account or the date to fix
      throw Exception(_reason(answer.body, answer.statusCode));
    }
    final body = jsonDecode(answer.body) as Map<String, dynamic>;
    final saved = body['tax'];
    if (saved is Map<String, dynamic>) return BookTax.fromJson(saved);
    return BookTax.fromJson(const {});
  }

  /// The usual taxes, posted to the accounts this Book already has. Never overwrites a rate the owner set.
  Future<TaxTable> seed() async {
    final answer = await _client.post(
      Uri.parse('$_base/taxes/seed'),
      headers: _headers,
      body: '{}',
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_reason(answer.body, answer.statusCode));
    }
    return read();
  }

  /// What each chosen tax would be on a document, in the order the Book declares, before anything is posted.
  ///
  /// [gross] is the total the customer or supplier sees, because that is the figure on a document: the tax is taken out
  /// of it rather than added on top, so a car sold for 100,000 has its tax taken out of the 100,000.
  Future<TaxPreview> preview({
    required String date,
    required String direction,
    required String gross,
    List<String> taxes = const [],
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/taxes/compute'),
      headers: _headers,
      body: jsonEncode({
        'date': date,
        'direction': direction,
        'gross': gross,
        'taxes': taxes,
      }),
    );
    if (answer.statusCode != 200) {
      throw Exception(_reason(answer.body, answer.statusCode));
    }
    return TaxPreview.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
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
