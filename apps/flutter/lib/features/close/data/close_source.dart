// The three readings the "Books in order" page needs, each from the door that owns it.
//
// The page takes this as an interface so a test can answer all three without a server, and so a check that fails can be shown
// on its own line instead of hiding the two that worked.
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';

abstract class BookCloseSource {
  /// The trial balance as at today, which is what says whether the ledger adds up.
  Future<TrialBalance> trialBalance();

  /// The journal, so the entries still in draft can be counted.
  Future<List<JournalEntry>> journal();

  /// The purchases ledger's own check of its control account against its suppliers.
  Future<PayablesCheck> suppliersControl();
}

class RealBookCloseSource implements BookCloseSource {
  RealBookCloseSource({
    AccountingApiService? accounting,
    PayablesService? payables,
    DateTime? today,
  }) : _accounting = accounting ?? AccountingApiService(),
       _payables = payables ?? PayablesService.instance,
       _today = today ?? DateTime.now();

  final AccountingApiService _accounting;
  final PayablesService _payables;
  final DateTime _today;

  @override
  Future<TrialBalance> trialBalance() =>
      _accounting.getTrialBalance(asOfDate: _today);

  @override
  Future<List<JournalEntry>> journal() => _accounting.getJournalEntries();

  @override
  Future<PayablesCheck> suppliersControl() => _payables.check();
}
