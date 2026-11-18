// The four figures a business owner opens the app for, each asked of the door that already owns it.
//
// Nothing is added up here that a door already adds up. Money held is the cash position's own total, what is owed is the
// invoice register's own outstanding sum, what the business owes is the purchases ledger's own total, and the profit is the
// income statement's own net figure. A second arithmetic on the phone is a second answer that can disagree with the screen
// it came from, and the two would drift apart on the first day nobody looked.
//
// The page takes this as an interface rather than reaching for four services itself, so a test can answer all four without a
// server and a failure on one can be shown on its own card instead of blanking the page.
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';

/// What the "Where you stand" page needs from the Book.
abstract class WhereYouStandSource {
  /// Money the business holds, in cash and at the bank.
  Future<ExactAmount> moneyHeld();

  /// What customers still have to pay on invoices already raised.
  Future<ExactAmount> owedToYou();

  /// What the business still has to pay its suppliers.
  Future<ExactAmount> youOwe();

  /// What the Book has made since the first day of the year, before drawings.
  Future<ExactAmount> profitThisYear();
}

/// The real thing: four doors, asked as they are.
class BookStandSource implements WhereYouStandSource {
  BookStandSource({
    CashBookService? cashBook,
    InvoicingService? invoicing,
    PayablesService? payables,
    AccountingApiService? accounting,
    DateTime? today,
  }) : _cashBook = cashBook ?? CashBookService(),
       _invoicing = invoicing ?? InvoicingService.instance,
       _payables = payables ?? PayablesService.instance,
       _accounting = accounting ?? AccountingApiService(),
       _today = today ?? DateTime.now();

  final CashBookService _cashBook;
  final InvoicingService _invoicing;
  final PayablesService _payables;
  final AccountingApiService _accounting;
  final DateTime _today;

  @override
  Future<ExactAmount> moneyHeld() async {
    final position = await _cashBook.position();
    // the three parts, added by the position itself so this cannot disagree with the cash book's own heading
    return position.total;
  }

  @override
  Future<ExactAmount> owedToYou() async {
    final held = await _invoicing.listInvoices();
    return owedAcross(held);
  }

  @override
  Future<ExactAmount> youOwe() async {
    final position = await _payables.position();
    return position.total;
  }

  @override
  Future<ExactAmount> profitThisYear() async {
    final start = DateTime(_today.year);
    final statement = await _accounting.getIncomeStatement(start, _today);
    return statement.netIncome;
  }
}
