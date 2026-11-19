// What a bill does to the books, decided in one place.
//
// Recording a bill against a supplier keeps the purchases ledger: it answers who is owed what, and when. It does not put
// anything into the accounts, so a Book whose bills all live in the purchases ledger shows no costs and no supplier debt
// on its own statements. This file works out the double entry that closes that gap, and refuses rather than guessing when
// the chart cannot carry it.
//
// The rule is the ordinary one, and it is here rather than in a screen so every route to posting a bill says the same
// thing: a bill received is Dr what it cost you, Cr what you owe the supplier. A bill that carries tax adds a third side,
// Dr the tax account, because that tax is reclaimable and folding it into the cost would both overstate the cost and lose
// the reclaim. A payment is not worked out here, because
// money leaving is a cash movement and the cash book already owns that entry, with its own reference.
//
// Nothing is hard-coded to an account number. A Book's chart is its own, so the two sides are found by type and name in
// that chart; a chart with no payable account, or no account of the right kind to charge, is refused with the fix named.
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/tax/data/taxed_amounts.dart';

/// One side of the entry a bill or a payment would post.
class ProposedSide {
  final String accountNumber;
  final String accountName;
  final ExactAmount debit;
  final ExactAmount credit;
  final String description;

  const ProposedSide({
    required this.accountNumber,
    required this.accountName,
    required this.debit,
    required this.credit,
    required this.description,
  });

  bool get isDebit => !debit.isZero;

  /// The line as it will be written.
  String get line =>
      '${isDebit ? 'Dr' : 'Cr'} $accountNumber $accountName  '
      '${(isDebit ? debit : credit).toDecimalString()}';
}

/// The entry a bill or a payment would post, or the reason it cannot be posted.
class BillPosting {
  final List<ProposedSide> sides;
  final String reference;
  final String description;

  /// Empty when the entry can be posted; otherwise the sentence to show instead of posting.
  final String refusal;

  const BillPosting({
    required this.sides,
    required this.reference,
    required this.description,
    this.refusal = '',
  });

  const BillPosting.refused(this.refusal)
    : sides = const [],
      reference = '',
      description = '';

  bool get canPost => refusal.isEmpty && sides.isNotEmpty;

  /// The total the two sides share, which is what a person checks before posting.
  ExactAmount get total => sides.isEmpty
      ? ExactAmount.zero
      : (sides.first.isDebit ? sides.first.debit : sides.first.credit);
}

/// Accounts of one kind, in chart order, so a picker and a default agree.
List<Account> accountsOfType(List<Account> chart, String type) => [
  for (final account in chart)
    if (account.accountType == type) account,
];

/// The account a Book keeps what it owes suppliers in.
///
/// It has to be a liability: "Accounts Payable" is the usual name, "Creditors" the other one in common use. A chart that
/// keeps supplier debt under some third name is not guessed at, because posting to the wrong liability would misstate
/// both it and the account that was meant.
Account? payableAccount(List<Account> chart) {
  for (final account in accountsOfType(chart, 'liability')) {
    final name = account.accountName.toLowerCase();
    if (name.contains('payable') || name.contains('creditor')) return account;
  }
  return null;
}

/// The entry for a bill that has been received and not yet recorded in the accounts.
///
/// [charged] is the account the cost belongs in. It is passed in rather than guessed when a chart holds several, because
/// only the person holding the bill knows whether it was rent, stock or stationery.
/// [tax] is the tax the bill carries, which is a fact printed on the bill rather than something worked out from a rate.
/// It is posted to the Book's own tax account as something to reclaim, so a taxed bill does not put its tax into the cost
/// of what was bought.
BillPosting billPosting({
  required List<Account> chart,
  required ExactAmount amount,
  required String supplierName,
  required String reference,
  Account? charged,
  ExactAmount tax = ExactAmount.zero,
}) {
  if (amount.isZero) {
    return const BillPosting.refused(
      'A bill of nothing changes nothing, so there is nothing to post.',
    );
  }
  if (amount.isNegative) {
    return const BillPosting.refused('A bill cannot be less than nothing.');
  }
  final owed = payableAccount(chart);
  if (owed == null) {
    return const BillPosting.refused(
      'This Book has no Accounts Payable account, so a bill cannot be posted. '
      'Add one in Chart of accounts.',
    );
  }
  final cost = charged ?? _defaultCharge(chart);
  if (cost == null) {
    return const BillPosting.refused(
      'This Book has no cost or expense account, so a bill cannot be posted. '
      'Add one in Chart of accounts.',
    );
  }
  final cleanReference = reference.trim();
  final whose = supplierName.trim().isEmpty
      ? 'a supplier'
      : supplierName.trim();
  if (!tax.isZero || tax.isNegative) {
    try {
      checkTaxFits(total: amount, tax: tax, document: 'bill');
    } on Exception catch (refusal) {
      return BillPosting.refused(
        refusal.toString().replaceFirst('Exception: ', ''),
      );
    }
  }
  // the tax is reclaimable, so it needs an account of its own; posting it into the cost would hide it
  final reclaim = taxAccountIn(chart);
  if (!tax.isZero && reclaim == null) {
    return const BillPosting.refused(
      'This bill carries tax, but the Book has no VAT account to reclaim it in, so it '
      'would be lost in the cost. Add one in Chart of accounts.',
    );
  }
  return BillPosting(
    reference: cleanReference,
    description: 'Bill from $whose',
    sides: [
      ProposedSide(
        accountNumber: cost.accountNumber,
        accountName: cost.accountName,
        debit: amount - tax,
        credit: ExactAmount.zero,
        description: 'Billed by $whose',
      ),
      if (!tax.isZero)
        ProposedSide(
          accountNumber: reclaim!.accountNumber,
          accountName: reclaim.accountName,
          debit: tax,
          credit: ExactAmount.zero,
          description: 'Tax on the bill from $whose',
        ),
      ProposedSide(
        accountNumber: owed.accountNumber,
        accountName: owed.accountName,
        debit: ExactAmount.zero,
        credit: amount,
        description: 'Owed to $whose',
      ),
    ],
  );
}

/// The cost account to charge when nobody has chosen one.
///
/// Purchases first, then expenses: a Book that keeps stock wants its bills there, and one that does not has only
/// expenses, so this picks the account a person would have picked themselves.
Account? _defaultCharge(List<Account> chart) {
  final costs = [
    for (final account in chart)
      if (account.accountType == 'expense' ||
          account.accountType == 'cost_of_sales')
        account,
  ];
  for (final account in costs) {
    final name = account.accountName.toLowerCase();
    if (name.contains('purchase')) return account;
  }
  for (final account in costs) {
    final name = account.accountName.toLowerCase();
    if (name.contains('expense') || name.contains('cost')) return account;
  }
  return costs.isEmpty ? null : costs.first;
}

/// Every account a bill's cost could be charged to, for a picker.
List<Account> chargeableAccounts(List<Account> chart) => [
  for (final account in chart)
    if (account.accountType == 'expense' ||
        account.accountType == 'cost_of_sales')
      account,
];
