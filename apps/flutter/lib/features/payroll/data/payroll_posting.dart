// Turning a payroll run's journal into accounts this Book actually has.
//
// The calculator says what a payroll moves by name - "Salaries Expense", "PAYE Payable", "Other Deductions Payable" - and
// a Book's chart says what it moves by number. The two are joined here, by name, so nothing is hard-coded to one chart:
// a business that calls its wage account "Wages and Salaries" still gets its payroll posted, and one whose chart lacks
// the accounts is told exactly which are missing before anything is written.
//
// The second half of the job is choosing a number for an account that is missing. This allocates a free one instead of
// guessing a fixed 6100 that a Book may already have spent: a chart with its own 6100 would otherwise end up with two
// accounts meaning different things, and a trial balance nobody can explain.
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/payroll/data/payroll_service.dart';

/// An account a run needs and the chart does not have.
class PayrollAccountNeed {
  /// What the calculator called it, which is the name the account is created with.
  final String name;

  /// `expense` or `liability`, following which side of the journal the line sits on.
  final String accountType;

  /// `debit` or `credit`, the side the account's balance ordinarily sits on.
  final String normalBalance;

  /// The number to try first; [freeAccountNumber] moves on from here if it is taken.
  final String seed;

  const PayrollAccountNeed({
    required this.name,
    required this.accountType,
    required this.normalBalance,
    required this.seed,
  });

  String get description => 'Payroll: $name';
}

/// Every account [lines] need, in the order the journal lists them, without repeats.
///
/// The type comes from the side the line moves rather than from a table of names: whatever a payroll debits is a cost to
/// the business and whatever it credits is something it now owes. That holds for the statutory payables, the pension and
/// medical accounts and the net-pay payable alike, and it keeps working when the calculator adds a line.
List<PayrollAccountNeed> neededAccounts(List<PayrollLine> lines) {
  final needs = <String, PayrollAccountNeed>{};
  for (final line in lines) {
    final name = line.account.trim();
    if (name.isEmpty) continue;
    final debited = !line.debit.isZero;
    needs.putIfAbsent(
      name.toLowerCase(),
      () => PayrollAccountNeed(
        name: name,
        accountType: debited ? 'expense' : 'liability',
        normalBalance: debited ? 'debit' : 'credit',
        seed: debited ? '6100' : '2100',
      ),
    );
  }
  return needs.values.toList();
}

/// The accounts a Book already has that answer to [needs], keyed by the name the calculator used.
///
/// Matching is by name, ignoring case and surrounding spaces, because that is the only thing the two sides share. A name
/// that matches nothing is simply absent from the answer, which is how a missing account is noticed.
Map<String, Account> matchAccounts(
  List<PayrollAccountNeed> needs,
  List<Account> chart,
) {
  final byName = <String, Account>{};
  for (final account in chart) {
    byName.putIfAbsent(account.accountName.trim().toLowerCase(), () => account);
  }
  final matched = <String, Account>{};
  for (final need in needs) {
    final account = byName[need.name.toLowerCase()];
    if (account != null) matched[need.name] = account;
  }
  return matched;
}

/// The needs that [matched] does not answer, in the order they were asked for.
List<PayrollAccountNeed> missingAccounts(
  List<PayrollAccountNeed> needs,
  Map<String, Account> matched,
) => [
  for (final need in needs)
    if (!matched.containsKey(need.name)) need,
];

/// A number near [seed] that [taken] does not use, so an account added now cannot collide with one already there.
///
/// It walks upwards in tens, which keeps related accounts together and leaves room between them; a chart holding every
/// number up to the top of its band is not a chart anybody can read, so nothing more clever than that is attempted.
String freeAccountNumber(String seed, Set<String> taken) {
  final start = int.tryParse(seed);
  if (start == null) return seed;
  var number = start;
  while (taken.contains(number.toString())) {
    number += 10;
  }
  return number.toString();
}
