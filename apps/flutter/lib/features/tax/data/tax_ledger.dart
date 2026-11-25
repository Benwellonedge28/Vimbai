// The tax ledger on the phone: each tax the Book charges, what it took in, what was taken back, and what was paid over.
//
// The screen above this one answers what is owed over a month. This is the account behind that figure, and the door the
// Book did not have: a tax that was recorded as owed could never be shown as paid, so the liability stood for ever and
// the balance sheet carried a debt the business had already settled. Paying it over is one entry - the tax account gives
// the liability up and the bank or cash it left records the payment - and what is owed afterwards is what the ledger says
// rather than what somebody remembers paying.
//
// Two things this file will not do. It will not apply a rate: every figure is read from the entries the Book holds. And it
// will not work out how much to pay: the ledger states what is owed and the person states what was actually paid.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One movement in a tax account, with what it did to the balance owed.
class TaxMovement {
  const TaxMovement({
    required this.entryDate,
    required this.description,
    required this.reference,
    required this.kind,
    required this.debit,
    required this.credit,
    required this.balance,
  });

  final String entryDate;
  final String description;
  final String reference;

  /// `charged`, `taken_back`, `paid` or `returned`.
  final String kind;

  final ExactAmount debit;
  final ExactAmount credit;
  final ExactAmount balance;

  factory TaxMovement.fromJson(Map<String, dynamic> json) => TaxMovement(
    entryDate: (json['entry_date'] as String?) ?? '',
    description: (json['description'] as String?) ?? '',
    reference: (json['reference'] as String?) ?? '',
    kind: (json['kind'] as String?) ?? 'charged',
    debit: ExactAmount.fromJson(json['debit']),
    credit: ExactAmount.fromJson(json['credit']),
    balance: ExactAmount.fromJson(json['balance']),
  );

  /// What this movement did, in the words a person would use.
  String get what => switch (kind) {
    'paid' => 'Paid over',
    'returned' => 'Payment undone',
    'taken_back' => 'Taken back',
    _ => 'Charged',
  };

  /// The figure this movement moved, whichever side it moved on.
  ExactAmount get moved => credit.isZero ? debit : credit;

  /// "Charged 150.00, 2026-10-05" and the reference when the Book kept one.
  String get reading {
    final named = reference.trim();
    return named.isEmpty
        ? '$what ${moved.toDecimalString()} on $entryDate'
        : '$what ${moved.toDecimalString()} on $entryDate, $named';
  }
}

/// One tax's account, as the Book kept it over the window.
class TaxLedgerAccount {
  const TaxLedgerAccount({
    required this.accountNumber,
    required this.accountName,
    required this.taxName,
    required this.rate,
    required this.opening,
    required this.charged,
    required this.takenBack,
    required this.paid,
    required this.returned,
    required this.closing,
    required this.isReclaimable,
    required this.movements,
  });

  final String accountNumber;
  final String accountName;

  /// The tax the Book defines against this account, when it defines one.
  final String taxName;

  /// The rate the Book states for that tax, as text: 15.5 is fifteen and a half per cent.
  final String rate;

  final ExactAmount opening;
  final ExactAmount charged;
  final ExactAmount takenBack;
  final ExactAmount paid;
  final ExactAmount returned;

  /// What is owed now. Below nothing means the tax authority owes the business.
  final ExactAmount closing;

  final bool isReclaimable;
  final List<TaxMovement> movements;

  factory TaxLedgerAccount.fromJson(Map<String, dynamic> json) =>
      TaxLedgerAccount(
        accountNumber: (json['account_number'] as String?) ?? '',
        accountName: (json['account_name'] as String?) ?? '',
        taxName: (json['tax_name'] as String?) ?? '',
        rate: (json['rate'] as String?) ?? '',
        opening: ExactAmount.fromJson(json['opening']),
        charged: ExactAmount.fromJson(json['charged']),
        takenBack: ExactAmount.fromJson(json['taken_back']),
        paid: ExactAmount.fromJson(json['paid']),
        returned: ExactAmount.fromJson(json['returned']),
        closing: ExactAmount.fromJson(json['closing']),
        isReclaimable: json['is_reclaimable'] == true,
        movements: [
          for (final row in (json['movements'] as List?) ?? const [])
            TaxMovement.fromJson(row as Map<String, dynamic>),
        ],
      );

  /// The name to lead with: the tax the Book defines, or what the chart calls the account.
  String get label {
    final tax = taxName.trim();
    if (tax.isNotEmpty) return tax;
    return accountName.trim().isEmpty ? accountNumber : accountName;
  }

  /// "2100 VAT Payable", under the name.
  String get place => accountNumber.trim().isEmpty
      ? accountName
      : '$accountNumber $accountName';

  /// The rate in words, when the Book states one: "15.5%".
  String get rateWords {
    final stated = rate.trim();
    if (stated.isEmpty) return '';
    // the engine states four decimal places; a rate that ends in zeros reads better without them
    var trimmed = stated;
    if (trimmed.contains('.')) {
      trimmed = trimmed.replaceFirst(RegExp(r'0+$'), '');
      trimmed = trimmed.replaceFirst(RegExp(r'\.$'), '');
    }
    return '$trimmed%';
  }

  /// What is owed, or what can be reclaimed, in the words a person would use.
  String get owedWords {
    if (isReclaimable) {
      return '${(ExactAmount.zero - closing).toDecimalString()} to reclaim';
    }
    if (closing.isZero) return 'Nothing owed';
    return '${closing.toDecimalString()} owed';
  }

  /// Whether there is a payment to make against this tax.
  bool get canBePaidOver => !closing.isZero && !isReclaimable;

  /// "Charged 150.00, taken back 60.00, paid over 0.00" over the window, leaving out what did not happen.
  String get moved {
    final parts = <String>[];
    if (!charged.isZero) parts.add('${charged.toDecimalString()} charged');
    if (!takenBack.isZero) {
      parts.add('${takenBack.toDecimalString()} taken back');
    }
    if (!paid.isZero) parts.add('${paid.toDecimalString()} paid over');
    if (!returned.isZero) parts.add('${returned.toDecimalString()} returned');
    if (parts.isEmpty) return 'Nothing moved in this period';
    return parts.join(', ');
  }
}

/// The whole tax ledger: every tax account the Book keeps, and what each comes to.
class TaxLedger {
  const TaxLedger({
    required this.from,
    required this.to,
    required this.accounts,
    required this.closing,
    required this.hasTaxAccount,
    required this.note,
  });

  final String from;
  final String to;
  final List<TaxLedgerAccount> accounts;

  /// What every tax account owes together.
  final ExactAmount closing;

  /// Whether the Book's chart holds an account whose name says VAT or tax. Without one nothing can have been posted, so
  /// a nil figure here is a missing account rather than a nil return.
  final bool hasTaxAccount;

  /// The engine's own sentence about what this ledger can and cannot say.
  final String note;

  factory TaxLedger.fromJson(Map<String, dynamic> json) => TaxLedger(
    from: (json['from'] as String?) ?? '',
    to: (json['to'] as String?) ?? '',
    accounts: [
      for (final row in (json['taxes'] as List?) ?? const [])
        TaxLedgerAccount.fromJson(row as Map<String, dynamic>),
    ],
    closing: ExactAmount.fromJson(
      (json['totals'] as Map<String, dynamic>?)?['closing'],
    ),
    hasTaxAccount: json['has_tax_account'] == true,
    note: (json['note'] as String?) ?? '',
  );

  /// What to say first.
  String get headline {
    if (!hasTaxAccount) return 'This Book has no tax account';
    if (accounts.isEmpty) return 'No tax is owed';
    if (closing.isZero) return 'Every tax is settled';
    if (closing.isNegative) {
      return '${(ExactAmount.zero - closing).toDecimalString()} to reclaim';
    }
    return '${closing.toDecimalString()} owed in tax';
  }

  /// The sentence under the headline.
  String get detail {
    if (!hasTaxAccount) {
      return 'Tax is posted to an account whose name says VAT or tax, and this Book has none, so nothing could have '
          'been recorded. Add one in Chart of accounts.';
    }
    final named = accounts.where((account) => !account.closing.isZero).length;
    if (named == 0) {
      return 'Every tax account stands at nothing for this period, so there is nothing to pay over.';
    }
    return '${named == 1 ? 'One tax account is' : '$named tax accounts are'} still owed '
        'something, between $from and $to.';
  }
}

/// What the engine answered when a payment was posted.
class TaxPayment {
  const TaxPayment({
    required this.entryDate,
    required this.amount,
    required this.taxAccountNumber,
    required this.taxName,
    required this.moneyAccountName,
    required this.reference,
    required this.owedAfter,
  });

  final String entryDate;
  final ExactAmount amount;
  final String taxAccountNumber;
  final String taxName;
  final String moneyAccountName;
  final String reference;
  final ExactAmount owedAfter;

  factory TaxPayment.fromJson(Map<String, dynamic> json) => TaxPayment(
    entryDate: (json['entry_date'] as String?) ?? '',
    amount: ExactAmount.fromJson(json['amount']),
    taxAccountNumber: (json['tax_account_number'] as String?) ?? '',
    taxName: (json['tax_name'] as String?) ?? '',
    moneyAccountName: (json['money_account_name'] as String?) ?? '',
    reference: (json['reference'] as String?) ?? '',
    owedAfter: ExactAmount.fromJson(json['owed_after']),
  );

  /// What the person is told once it is posted.
  String get reading =>
      '${amount.toDecimalString()} of $taxName paid out of $moneyAccountName on $entryDate. '
      '${owedAfter.isZero ? 'Nothing is owed on it now.' : '${owedAfter.toDecimalString()} still owed.'}';
}

/// The door that answers the tax ledger and posts a payment against it.
class TaxLedgerService {
  TaxLedgerService._();

  static final TaxLedgerService instance = TaxLedgerService._();

  /// A second instance for a test that needs a transport of its own.
  TaxLedgerService.fresh();

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

  /// Every tax account between two days, both ends included.
  Future<TaxLedger> read({required String from, required String to}) async {
    final answer = await _client.get(
      Uri.parse('$_base/tax-ledger?from=$from&to=$to'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(_refusalOf(answer));
    }
    return TaxLedger.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }

  /// Pay [amount] of [taxAccountNumber] over on [entryDate], out of the account named or the Book's own bank or cash.
  Future<TaxPayment> payOver({
    required String entryDate,
    required String taxAccountNumber,
    required String amount,
    String period = '',
    String moneyAccountNumber = '',
  }) async {
    final answer = await _client.post(
      Uri.parse('$_base/tax-ledger/settle'),
      headers: _headers,
      body: jsonEncode({
        'entry_date': entryDate.trim(),
        'tax_account_number': taxAccountNumber.trim(),
        'amount': amount.trim(),
        if (period.trim().isNotEmpty) 'period': period.trim(),
        if (moneyAccountNumber.trim().isNotEmpty)
          'money_account_number': moneyAccountNumber.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return TaxPayment.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }

  /// The engine's own sentence when it refuses, rather than a status code and a wall of JSON.
  String _refusalOf(http.Response answer) {
    try {
      final body = jsonDecode(answer.body);
      if (body is Map<String, dynamic>) {
        for (final key in ['detail', 'message', 'error']) {
          final words = body[key];
          if (words is String && words.trim().isNotEmpty) return words;
        }
      }
    } catch (_) {
      // the body was not the engine's envelope, so the status is all there is to say
    }
    return 'The engine refused this: ${answer.statusCode}.';
  }
}
