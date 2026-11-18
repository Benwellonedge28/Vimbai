// The cash and bank book: the money the business holds, and everything that moved in or out of it.
//
// This is the other half of invoicing. An invoice says what has been billed; the cash book says what is actually there, and
// the engine moves an account's balance the moment an entry is written - so an entry here is money that has moved, not a
// note that it might. Nothing is computed on the phone: balances come from the engine, in exact text, because a balance the
// app worked out for itself would disagree with the one the books keep.
import 'dart:convert';

import 'package:http/http.dart' as http;

import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One cash or bank account, with the balance the engine holds for it.
class CashAccount {
  final String id;
  final String code;
  final String name;

  /// `cash`, `bank`, `savings` - the engine's own word, shown as it is.
  final String type;
  final String bankName;
  final String currency;
  final ExactAmount openingBalance;
  final ExactAmount balance;

  const CashAccount({
    required this.id,
    required this.code,
    required this.name,
    required this.type,
    required this.bankName,
    required this.currency,
    required this.openingBalance,
    required this.balance,
  });

  factory CashAccount.fromJson(Map<String, dynamic> json) => CashAccount(
    id: (json['id'] as String?) ?? (json['account_code'] as String?) ?? '',
    code: (json['account_code'] as String?) ?? '',
    name: (json['account_name'] as String?) ?? '',
    type: (json['account_type'] as String?) ?? 'bank',
    bankName: (json['bank_name'] as String?) ?? '',
    currency: (json['currency'] as String?) ?? 'USD',
    openingBalance: ExactAmount.fromJson(json['opening_balance']),
    balance: ExactAmount.fromJson(json['current_balance']),
  );
}

/// One receipt or payment against a cash or bank account.
class CashEntry {
  final String id;
  final String date;
  final String voucher;
  final String description;

  /// True when the money came in, false when it went out.
  final bool moneyIn;
  final ExactAmount amount;
  final String status;

  /// The chart account the other side of this entry belongs to, as the person named it.
  final String againstAccount;

  const CashEntry({
    required this.id,
    required this.date,
    required this.voucher,
    required this.description,
    required this.moneyIn,
    required this.amount,
    required this.status,
    this.againstAccount = '',
  });

  factory CashEntry.fromJson(Map<String, dynamic> json) => CashEntry(
    id: (json['id'] as String?) ?? '',
    date: (json['entry_date'] as String?) ?? '',
    voucher: (json['voucher_number'] as String?) ?? '',
    description: (json['description'] as String?) ?? '',
    moneyIn: json['is_debit'] == true,
    amount: ExactAmount.fromJson(json['amount']),
    status: (json['status'] as String?) ?? '',
    againstAccount: (json['account_code'] as String?) ?? '',
  );

  /// The signed effect on the balance, for adding up a page of entries.
  ExactAmount get effect => moneyIn ? amount : amount.negated();
}

/// How much is held, in total and per account, as the engine reports it.
///
/// The engine's `total_current` is the money in *current* accounts only, not everything held - a Book whose whole balance sat
/// in one bank account answered 0.00 for it - so the total here is the three parts added up rather than that field read.
/// The parts are the engine's own figures, so this adds its answers, it does not work out balances for itself.
class CashPosition {
  final ExactAmount cash;
  final ExactAmount bank;
  final ExactAmount savings;

  const CashPosition({
    required this.cash,
    required this.bank,
    required this.savings,
  });

  factory CashPosition.fromJson(Map<String, dynamic> json) => CashPosition(
    cash: ExactAmount.fromJson(json['total_cash']),
    bank: ExactAmount.fromJson(json['total_bank']),
    savings: ExactAmount.fromJson(json['total_savings']),
  );

  ExactAmount get total => cash + bank + savings;
}

/// One month's check of a cash or bank account against the statement the bank sent.
///
/// The engine does the arithmetic: it adds the account's opening balance to its posted entries for the book side, and keeps
/// the statement balance and the adjustments as given, so the record shows what the two sides said rather than what the phone
/// thought. The difference is kept here as books minus statement: negative means the bank shows money the books have not
/// recorded yet, positive means the books show money the bank has not seen.
class CashReconciliation {
  final String id;
  final String accountCode;
  final String statementDate;
  final ExactAmount statementBalance;
  final ExactAmount bookBalance;
  final String status;
  final String reviewedBy;

  const CashReconciliation({
    required this.id,
    required this.accountCode,
    required this.statementDate,
    required this.statementBalance,
    required this.bookBalance,
    required this.status,
    required this.reviewedBy,
  });

  factory CashReconciliation.fromJson(Map<String, dynamic> json) =>
      CashReconciliation(
        id: (json['id'] as String?) ?? '',
        accountCode: (json['bank_account'] as String?) ?? '',
        statementDate: (json['statement_date'] as String?) ?? '',
        statementBalance: ExactAmount.fromJson(json['statement_balance']),
        bookBalance: ExactAmount.fromJson(json['book_balance']),
        status: (json['status'] as String?) ?? '',
        reviewedBy: (json['reviewed_by'] as String?) ?? '',
      );

  /// What the books show beyond what the bank shows.
  ExactAmount get gap => bookBalance - statementBalance;

  bool get agrees => gap.isZero;
}

class CashBookService {
  CashBookService({http.Client? client, String? base})
    : _client = client ?? http.Client(),
      _base = base ?? AppConfig.cashbookRoute;

  final http.Client _client;
  final String _base;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.value != null)
      'Authorization': 'Bearer ${SessionToken.value}',
    ...BookContext.instance.headers(),
  };

  Future<List<CashAccount>> accounts() async {
    final answer = await _client.get(
      Uri.parse('$_base/accounts'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return [
      for (final row in jsonDecode(answer.body) as List)
        CashAccount.fromJson(row as Map<String, dynamic>),
    ];
  }

  Future<CashPosition> position() async {
    final answer = await _client.get(
      Uri.parse('$_base/cash-position'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return CashPosition.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// The entries against one account, newest first as the engine sorts them.
  Future<List<CashEntry>> entries(String accountCode) async {
    final answer = await _client.get(
      Uri.parse(
        '$_base/entries?bank_account=${Uri.encodeQueryComponent(accountCode)}',
      ),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return [
      for (final row in jsonDecode(answer.body) as List)
        CashEntry.fromJson(row as Map<String, dynamic>),
    ];
  }

  Future<CashAccount> addAccount({
    required String code,
    required String name,
    required String type,
    String bankName = '',
    ExactAmount opening = ExactAmount.zero,
  }) async {
    if (code.trim().isEmpty || name.trim().isEmpty) {
      throw Exception('A cash account needs a code and a name.');
    }
    final answer = await _client.post(
      Uri.parse('$_base/accounts'),
      headers: _headers,
      body: jsonEncode({
        'account_code': code.trim(),
        'account_name': name.trim(),
        'account_type': type,
        if (bankName.trim().isNotEmpty) 'bank_name': bankName.trim(),
        'opening_balance': opening.toLedgerString(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(
        'The engine refused the account: ${answer.statusCode} ${answer.body}',
      );
    }
    return CashAccount.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// Record money in or out. The engine moves the balance as it writes the entry.
  Future<CashEntry> record({
    required CashAccount account,
    required bool moneyIn,
    required ExactAmount amount,
    required String description,
    required String date,
    String voucher = '',
    String againstAccount = '',
  }) async {
    if (amount.isZero) {
      throw Exception('An entry has to be more than nothing.');
    }
    if (amount.isNegative) {
      throw Exception(
        'Enter a positive figure and choose money in or money out.',
      );
    }
    final answer = await _client.post(
      Uri.parse('$_base/entries'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': date,
        'voucher_number': voucher,
        'description': description.trim(),
        'account_code': againstAccount.trim(),
        'amount': amount.toLedgerString(),
        'is_debit': moneyIn,
        'bank_account': account.code,
        'book_type': moneyIn ? 'receipts' : 'payments',
        // Written as posted, not left waiting. The engine moves the account's balance when the entry is created whatever
        // its status, but its own statement and any reconciliation count only posted entries - so an entry left pending had
        // the account's balance and the account's statement disagreeing about the same money.
        'status': 'posted',
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(
        'The engine refused the entry: ${answer.statusCode} ${answer.body}',
      );
    }
    return CashEntry.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }

  /// Every reconciliation kept for one account, newest statement first.
  Future<List<CashReconciliation>> reconciliations(String accountCode) async {
    final answer = await _client.get(
      Uri.parse(
        '$_base/reconciliations?bank_account=${Uri.encodeQueryComponent(accountCode)}',
      ),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return [
      for (final row in jsonDecode(answer.body) as List)
        CashReconciliation.fromJson(row as Map<String, dynamic>),
    ];
  }

  /// Check one account against the statement the bank sent.
  Future<CashReconciliation> reconcile({
    required CashAccount account,
    required String statementDate,
    required ExactAmount statementBalance,
    String preparedBy = '',
  }) async {
    if (statementDate.trim().isEmpty) {
      throw Exception('A reconciliation needs the date of the statement.');
    }
    final answer = await _client.post(
      Uri.parse('$_base/reconciliations'),
      headers: _headers,
      body: jsonEncode({
        'bank_account': account.code,
        'statement_date': statementDate.trim(),
        'statement_balance': statementBalance.toLedgerString(),
        if (preparedBy.trim().isNotEmpty) 'prepared_by': preparedBy.trim(),
        'adjustments': const [],
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(
        'The engine refused the reconciliation: ${answer.statusCode} ${answer.body}',
      );
    }
    return CashReconciliation.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// Close a reconciliation that has been looked at and accepted.
  Future<CashReconciliation> markReconciled(
    CashReconciliation reconciliation, {
    required String reviewedBy,
    required String completedAt,
  }) async {
    if (reviewedBy.trim().isEmpty) {
      throw Exception('Say who checked it.');
    }
    final answer = await _client.put(
      Uri.parse('$_base/reconciliations/${reconciliation.id}/complete'),
      headers: _headers,
      body: jsonEncode({
        'reviewed_by': reviewedBy.trim(),
        'completed_at': completedAt,
      }),
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine refused to close it: ${answer.statusCode} ${answer.body}',
      );
    }
    return CashReconciliation.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  void close() => _client.close();
}
