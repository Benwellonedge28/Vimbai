// What a sale owes the taxman, and what it earns.
//
// The app posted a sale as Dr receivable / Cr sales for the whole invoice, so a taxed invoice made the business look as
// though it had earned the tax and owed nobody anything. The engine now posts the tax as its own line; this file decides
// the three lines the phone sends, so the same split is used everywhere a sale is posted from here and can be tested
// without a server.
//
// Nothing here knows a rate. A tax rate is a fact about somebody's registration, not something an app may assume, so the
// amount of tax is the one the person's own invoice carries.
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/tax/data/taxed_amounts.dart';

export 'package:vimbai_mobile_client/features/tax/data/taxed_amounts.dart'
    show PostingAccount, taxAccountIn;

/// The three sides of a sale that carries tax.
class SaleSplit {
  const SaleSplit({
    required this.owed,
    required this.earned,
    required this.taxAccount,
    required this.tax,
    required this.total,
  });

  /// The receivable: the customer owes the whole invoice.
  final PostingAccount owed;

  /// The revenue: what the business actually earned.
  final PostingAccount earned;

  /// Where the tax sits, which is the Book's own account rather than a number this app chose.
  final PostingAccount taxAccount;

  final ExactAmount tax;
  final ExactAmount total;

  /// What the sale earns once the tax is taken out of it.
  ExactAmount get revenue => total - tax;

  bool get carriesTax => !tax.isZero;

  /// The lines, in the order a person reads a journal: debit first, then the credits.
  ///
  /// [reference] is the invoice number, which is what makes a second attempt at the same sale a refusal rather than a
  /// second posting, and which is how a line in the ledger is traced back to the document it came from.
  List<JournalLine> linesFor(String reference) => [
    JournalLine(
      accountNumber: owed.accountNumber,
      debit: total,
      credit: ExactAmount.zero,
      description: 'Owed on $reference',
    ),
    JournalLine(
      accountNumber: earned.accountNumber,
      debit: ExactAmount.zero,
      credit: revenue,
      description: 'Sold on $reference',
    ),
    if (carriesTax)
      JournalLine(
        accountNumber: taxAccount.accountNumber,
        debit: ExactAmount.zero,
        credit: tax,
        description: 'Tax on $reference',
      ),
  ];

  /// The two or three lines as a person reads them before saying yes.
  List<String> get reading => [
    'Dr ${owed.label}  ${total.toDecimalString()}',
    'Cr ${earned.label}  ${revenue.toDecimalString()}',
    if (carriesTax) 'Cr ${taxAccount.label}  ${tax.toDecimalString()}',
  ];
}

/// Build the split, refusing a tax the invoice cannot support.
///
/// The refusals are sentences rather than silent corrections, because both mistakes here are the kind a person can see in
/// a moment and cannot see afterwards: tax larger than the invoice, and tax equal to it, which would leave a sale that
/// earned nothing.
SaleSplit saleSplit({
  required PostingAccount owed,
  required PostingAccount earned,
  required PostingAccount taxAccount,
  required ExactAmount total,
  required ExactAmount tax,
}) {
  checkTaxFits(total: total, tax: tax, document: 'invoice');
  if (taxAccount.accountNumber == earned.accountNumber) {
    throw Exception(
      'The tax would be posted to the same account as the sale, which would fold the two together.',
    );
  }
  return SaleSplit(
    owed: owed,
    earned: earned,
    taxAccount: taxAccount,
    tax: tax,
    total: total,
  );
}
