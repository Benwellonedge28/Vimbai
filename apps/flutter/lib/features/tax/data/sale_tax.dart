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
import 'package:vimbai_mobile_client/features/tax/data/tax_table.dart';
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

/// The two lines a sale that carries no tax posts: the customer owes the whole figure, and the business earned it.
List<JournalLine> untaxedSaleLines({
  required PostingAccount owed,
  required PostingAccount earned,
  required ExactAmount total,
  required String reference,
}) => [
  JournalLine(
    accountNumber: owed.accountNumber,
    debit: total,
    credit: ExactAmount.zero,
    description: 'Owed on $reference',
  ),
  JournalLine(
    accountNumber: earned.accountNumber,
    debit: ExactAmount.zero,
    credit: total,
    description: 'Sold on $reference',
  ),
];

/// The lines a sale posts, taken from what the engine said the sale carries.
///
/// The engine works each tax out of the total from the tax's own percentage and names the account that tax belongs to, so
/// nothing here knows a rate or chooses an account: this only lays the engine's figures on the two sides a journal has.
/// One line per tax, which is the whole point of the tax table: each tax is owed somewhere of its own.
List<JournalLine> saleLinesFor({
  required TaxPreview preview,
  required PostingAccount owed,
  required PostingAccount earned,
  required String reference,
}) {
  if (preview.carriesNothing) {
    return untaxedSaleLines(
      owed: owed,
      earned: earned,
      total: preview.gross,
      reference: reference,
    );
  }
  return [
    JournalLine(
      accountNumber: owed.accountNumber,
      debit: preview.gross,
      credit: ExactAmount.zero,
      description: 'Owed on $reference',
    ),
    JournalLine(
      accountNumber: earned.accountNumber,
      debit: ExactAmount.zero,
      credit: preview.gross,
      description: 'Sold on $reference',
    ),
    for (final line in preview.lines)
      if (!line.amount.isZero) ...[
        JournalLine(
          accountNumber: line.expenseAccount,
          debit: line.amount,
          credit: ExactAmount.zero,
          description: '${line.tax} charged on $reference',
        ),
        JournalLine(
          accountNumber: line.accountNumber,
          debit: ExactAmount.zero,
          credit: line.amount,
          description: '${line.tax} owed on $reference',
        ),
      ],
  ];
}

/// What the phone asks the engine about tax on a sale, so a test can answer without a server.
abstract class SaleTaxReads {
  /// The Book's taxes that hold on [date] and are charged on sales.
  Future<List<BookTax>> chargedOnSales(String date);

  /// What the chosen taxes come to on a sale of [gross], in the Book's declared order.
  Future<TaxPreview> preview({
    required String date,
    required String gross,
    required List<String> taxes,
    String discount = '0',
  });
}

/// The engine's own answer, through the tax table it publishes.
class EngineSaleTax implements SaleTaxReads {
  EngineSaleTax({TaxTableService? table})
    : _table = table ?? TaxTableService.instance;

  final TaxTableService _table;

  @override
  Future<List<BookTax>> chargedOnSales(String date) async {
    final table = await _table.read(date: date);
    return [
      for (final tax in table.taxes)
        if (tax.inForce && tax.onSales) tax,
    ];
  }

  @override
  Future<TaxPreview> preview({
    required String date,
    required String gross,
    required List<String> taxes,
    String discount = '0',
  }) => _table.preview(
    date: date,
    direction: 'sale',
    gross: gross,
    taxes: taxes,
    discount: discount,
  );
}
