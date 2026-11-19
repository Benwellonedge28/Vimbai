// The parts of tax that a sale and a bill share.
//
// A taxed document posts its tax as its own line whichever way the money goes, and the tax account is the Book's own: it
// is found in the chart by name, never by a number this app chose. Both the sales path and the payables path come here, so
// the account is chosen once and the same sentences refuse the same mistakes.
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';

/// One account a document posts to, as the chart holds it.
class PostingAccount {
  const PostingAccount({
    required this.accountNumber,
    required this.accountName,
  });

  final String accountNumber;
  final String accountName;

  /// How the account reads in a confirmation: "2100 VAT Payable".
  String get label => '$accountNumber $accountName';

  factory PostingAccount.of(Account account) => PostingAccount(
    accountNumber: account.accountNumber,
    accountName: account.accountName,
  );
}

/// The account a Book keeps tax in, found by its name.
///
/// The engine finds it the same way, so the phone can never post tax to an account the engine would not have chosen. A
/// liability is preferred because collected tax is money owed, but a Book that keeps its reclaim in an asset account is
/// still found: the name is the test, not the type. Returns null when the chart holds no such account, which is a Book
/// that has never been able to record tax rather than a Book that owes none.
Account? taxAccountIn(List<Account> chart) {
  Account? named(String wanted) {
    for (final account in chart) {
      if (account.accountName.toLowerCase().contains(wanted)) return account;
    }
    return null;
  }

  final namedVat = named('vat') ?? named('tax');
  if (namedVat == null) return null;
  if (namedVat.accountType == 'liability') return namedVat;
  // a Book holding tax under "Tax Reclaimable" as an asset still answers to its name
  return namedVat;
}

/// Refuse a tax the document cannot carry.
///
/// [document] is the word the person used for it, so the sentence names their own document. Both refusals here are the
/// kind of mistake that is obvious for a moment and invisible afterwards: tax larger than the document, and tax equal to
/// it, which would leave a document that moved no money of its own.
void checkTaxFits({
  required ExactAmount total,
  required ExactAmount tax,
  required String document,
}) {
  if (tax.isNegative) {
    throw Exception('The tax on this $document cannot be less than nothing.');
  }
  if (tax > total) {
    throw Exception(
      'The tax of ${tax.toDecimalString()} is more than the '
      '${total.toDecimalString()} $document comes to. Check which figure is the tax.',
    );
  }
  if (!tax.isZero && tax == total) {
    throw Exception(
      'The whole ${total.toDecimalString()} was given as tax, so the $document would carry nothing '
      'but tax. Give the tax on its own, or nothing if it carries none.',
    );
  }
}
