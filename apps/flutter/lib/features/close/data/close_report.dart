// Is the Book in order: the three checks a person wants before they trust a set of accounts.
//
// Every one of these is already answered by a door - the trial balance says whether debits equal credits, the journal says
// what is still a draft, the purchases ledger says whether its control account agrees with the suppliers it stands for. This
// file only turns those three answers into a verdict and a sentence, which is what makes it testable without a server and what
// keeps the page from inventing arithmetic of its own.
//
// It is deliberately not a score. A Book is either in order or it is not, and when it is not the item says what is wrong in
// the engine's own figures so the person can go and fix it.
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';

enum CloseVerdict {
  /// The check ran and the answer is clean.
  inOrder,

  /// The check ran and found something to fix.
  needsYou,

  /// The check could not be made, which is said rather than counted as clean.
  couldNotCheck,
}

/// One check, with the verdict and the sentence a person reads.
class CloseItem {
  const CloseItem({
    required this.subject,
    required this.verdict,
    required this.headline,
    required this.detail,
  });

  /// A check that could not be made, saying what was not read and why.
  factory CloseItem.couldNotCheck({
    required String subject,
    required String what,
    required String reason,
  }) => CloseItem(
    subject: subject,
    verdict: CloseVerdict.couldNotCheck,
    headline: 'This check could not be made just now.',
    detail: '$what $reason',
  );

  /// What is being checked, as it is named to the person: "Debits and credits".
  final String subject;

  final CloseVerdict verdict;

  /// The answer in one line, with the engine's own figures in it.
  final String headline;

  /// Why it matters, in the words a person uses.
  final String detail;

  bool get needsYou => verdict == CloseVerdict.needsYou;

  /// The three checks of a Book, in the order they are worth fixing: a draft is money missing from every statement, a control
  /// account that disagrees is a supplier who will not be paid right, and a trial balance that does not balance is the whole
  /// ledger saying it does not add up.
  static CloseItem debitsAndCredits(TrialBalance? balance, {String? reason}) {
    if (balance == null) {
      return CloseItem.couldNotCheck(
        subject: 'Debits and credits',
        what:
            'The trial balance could not be read, so whether the ledger adds up is not known from here.',
        reason: reason ?? 'no reason given',
      );
    }
    final debits = balance.totalDebits;
    final credits = balance.totalCredits;
    if (balance.isBalanced) {
      return CloseItem(
        subject: 'Debits and credits',
        verdict: CloseVerdict.inOrder,
        headline:
            'The ledger balances: ${debits.toDecimalString()} on both sides.',
        detail: 'Every posting has an equal and opposite one behind it.',
      );
    }
    final gap = debits > credits ? debits - credits : credits - debits;
    return CloseItem(
      subject: 'Debits and credits',
      verdict: CloseVerdict.needsYou,
      headline:
          'Debits come to ${debits.toDecimalString()} and credits to '
          '${credits.toDecimalString()}, a difference of ${gap.toDecimalString()}.',
      detail:
          'A ledger that does not balance cannot be trusted for any statement taken from it.',
    );
  }

  static CloseItem drafts(Iterable<JournalEntry>? entries, {String? reason}) {
    if (entries == null) {
      return CloseItem.couldNotCheck(
        subject: 'Entries still in draft',
        what:
            'The journal could not be read, so entries left in draft would not be seen from here.',
        reason: reason ?? 'no reason given',
      );
    }
    final held = entries.toList();
    final drafts = held
        .where((entry) => (entry.status ?? '') == 'draft')
        .length;
    if (drafts == 0) {
      return CloseItem(
        subject: 'Entries still in draft',
        verdict: CloseVerdict.inOrder,
        headline: held.isEmpty
            ? 'The journal is empty, so nothing is waiting to be posted.'
            : 'All ${held.length} entries in the journal are posted.',
        detail: 'Nothing is sitting in draft where a statement cannot see it.',
      );
    }
    return CloseItem(
      subject: 'Entries still in draft',
      verdict: CloseVerdict.needsYou,
      headline:
          '$drafts ${drafts == 1 ? 'entry is' : 'entries are'} still in draft.',
      detail:
          'A draft moves nothing: until it is posted, it is missing from the trial balance, the statements and the ledger.',
    );
  }

  static CloseItem suppliersControl(PayablesCheck? check, {String? reason}) {
    if (check == null) {
      return CloseItem.couldNotCheck(
        subject: 'Suppliers and their control account',
        what:
            'The purchases ledger could not be checked, so whether it agrees with the ledger is not known from here.',
        reason: reason ?? 'no reason given',
      );
    }
    if (check.difference.isZero) {
      return CloseItem(
        subject: 'Suppliers and their control account',
        verdict: CloseVerdict.inOrder,
        headline:
            'The purchases control account agrees at '
            '${check.controlAccount.toDecimalString()}.',
        detail: 'What the ledger says is owed matches what the suppliers say.',
      );
    }
    return CloseItem(
      subject: 'Suppliers and their control account',
      verdict: CloseVerdict.needsYou,
      headline:
          'The control account says ${check.controlAccount.toDecimalString()} '
          'and the suppliers come to ${check.supplierTotal.toDecimalString()}, '
          'a difference of ${check.difference.toDecimalString()}.',
      detail:
          'The two should agree. A gap means a payment or a bill reached one side and not the other.',
    );
  }
}

/// The answer to "is this Book in order", as the three checks stand.
class CloseReport {
  const CloseReport(this.items);

  final List<CloseItem> items;

  int get needsYou => items.where((item) => item.needsYou).length;

  int get couldNotCheck =>
      items.where((item) => item.verdict == CloseVerdict.couldNotCheck).length;

  int get inOrder =>
      items.where((item) => item.verdict == CloseVerdict.inOrder).length;

  /// Everything clean, which is the only state worth calling in order.
  bool get isInOrder => needsYou == 0 && couldNotCheck == 0;

  /// The one to fix first: drafts before a control gap, and a control gap before the whole ledger not adding up.
  CloseItem? get firstNeedingYou {
    for (final item in items) {
      if (item.needsYou) return item;
    }
    return null;
  }
}
