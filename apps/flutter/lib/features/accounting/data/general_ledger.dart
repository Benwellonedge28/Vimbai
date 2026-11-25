// The general ledger: the whole Book for one window.
//
// One account's history already answered on its own screen, and the trial balance already listed every account's closing
// figure. What was missing is the report an accountant asks for by name: every account with what it opened on, what moved
// it, and what it closed on, side by side, with the proof that the window debited what it credited.
//
// Nothing is added up here. The engine answers the report and this reads it as written, so a figure on the phone and a
// figure in the engine cannot differ.
import 'package:vimbai_mobile_client/core/api/amount.dart';

/// One movement inside the window: a line of a posted entry that touched this account.
class GeneralLedgerRow {
  const GeneralLedgerRow({
    required this.entryId,
    required this.entryDate,
    required this.description,
    required this.debit,
    required this.credit,
    required this.balance,
    required this.sourceModule,
  });

  final String entryId;
  final String entryDate;
  final String description;
  final ExactAmount debit;
  final ExactAmount credit;

  /// The account's balance after this line, as the engine walked it.
  final ExactAmount balance;
  final String sourceModule;

  factory GeneralLedgerRow.fromJson(Map<String, dynamic> json) =>
      GeneralLedgerRow(
        entryId: (json['entry_id'] as String?) ?? '',
        entryDate: (json['entry_date'] as String?) ?? '',
        description: (json['description'] as String?) ?? '',
        debit: ExactAmount.fromJson(json['debit']),
        credit: ExactAmount.fromJson(json['credit']),
        balance: ExactAmount.fromJson(json['balance']),
        sourceModule: (json['source_module'] as String?) ?? '',
      );
}

/// One account over the window: what it opened on, what moved it, what it closed on.
class GeneralLedgerAccount {
  const GeneralLedgerAccount({
    required this.accountNumber,
    required this.accountName,
    required this.accountType,
    required this.normalBalance,
    required this.startBalance,
    required this.totalDebit,
    required this.totalCredit,
    required this.endBalance,
    required this.entries,
  });

  final String accountNumber;
  final String accountName;
  final String accountType;
  final String normalBalance;
  final ExactAmount startBalance;
  final ExactAmount totalDebit;
  final ExactAmount totalCredit;
  final ExactAmount endBalance;
  final List<GeneralLedgerRow> entries;

  /// Whether the account moved at all inside the window.
  bool get moved => entries.isNotEmpty;

  factory GeneralLedgerAccount.fromJson(Map<String, dynamic> json) =>
      GeneralLedgerAccount(
        accountNumber: (json['account_number'] as String?) ?? '',
        accountName: (json['account_name'] as String?) ?? '',
        accountType: (json['account_type'] as String?) ?? '',
        normalBalance: (json['normal_balance'] as String?) ?? '',
        startBalance: ExactAmount.fromJson(json['start_balance']),
        totalDebit: ExactAmount.fromJson(json['total_debit']),
        totalCredit: ExactAmount.fromJson(json['total_credit']),
        endBalance: ExactAmount.fromJson(json['end_balance']),
        entries: ((json['entries'] as List<dynamic>?) ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(GeneralLedgerRow.fromJson)
            .toList(growable: false),
      );
}

/// The report as the engine answered it.
class GeneralLedgerReport {
  const GeneralLedgerReport({
    required this.startDate,
    required this.endDate,
    required this.accounts,
    required this.accountCount,
    required this.movementCount,
    required this.totalDebit,
    required this.totalCredit,
    required this.isBalanced,
  });

  /// Empty when the window has no start, which is what the engine says rather than inventing a date.
  final String startDate;
  final String endDate;
  final List<GeneralLedgerAccount> accounts;
  final int accountCount;
  final int movementCount;
  final ExactAmount totalDebit;
  final ExactAmount totalCredit;

  /// Whether the window debited what it credited. False is a fact worth printing, never something to round away.
  final bool isBalanced;

  /// The window in words, for a report that ran without being given one.
  String get windowLabel {
    if (startDate.isEmpty && endDate.isEmpty) {
      return 'Everything the Book holds';
    }
    if (endDate.isEmpty) return 'From ${_day(startDate)}';
    if (startDate.isEmpty) return 'Up to ${_day(endDate)}';
    return '${_day(startDate)} to ${_day(endDate)}';
  }

  /// How much the report covers, in words: "2 accounts, 1 movement".
  String get summaryLabel {
    final accountWord = accountCount == 1 ? 'account' : 'accounts';
    final movementWord = movementCount == 1 ? 'movement' : 'movements';
    return '$accountCount $accountWord, $movementCount $movementWord';
  }

  static String _day(String date) =>
      date.length >= 10 ? date.substring(0, 10) : date;

  factory GeneralLedgerReport.fromJson(Map<String, dynamic> json) =>
      GeneralLedgerReport(
        startDate: (json['start_date'] as String?) ?? '',
        endDate: (json['end_date'] as String?) ?? '',
        accounts: ((json['accounts'] as List<dynamic>?) ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(GeneralLedgerAccount.fromJson)
            .toList(growable: false),
        accountCount: (json['account_count'] as num?)?.toInt() ?? 0,
        movementCount: (json['movement_count'] as num?)?.toInt() ?? 0,
        totalDebit: ExactAmount.fromJson(json['total_debit']),
        totalCredit: ExactAmount.fromJson(json['total_credit']),
        isBalanced: json['is_balanced'] == true,
      );
}
