import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/auth_service.dart';
import 'package:vimbai_mobile_client/services/book_context.dart';
import 'package:vimbai_mobile_client/models/book_models.dart';
import 'package:vimbai_mobile_client/services/book_sync_service.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/pages/login_page.dart';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:vimbai_mobile_client/services/accounting_api_service.dart'; // NEW
import 'package:vimbai_mobile_client/pages/multimodal_input_page.dart';
import 'package:vimbai_mobile_client/pages/books_page.dart';
import 'package:vimbai_mobile_client/pages/npo_page.dart';
import 'package:vimbai_mobile_client/pages/personal_finance_page.dart';
import 'package:vimbai_mobile_client/pages/bank_accounts_page.dart';
import 'package:vimbai_mobile_client/pages/financial_ratios_page.dart';
import 'package:vimbai_mobile_client/pages/journal_entries_list_page.dart';
import 'package:vimbai_mobile_client/pages/chart_of_accounts_page.dart';
import 'package:vimbai_mobile_client/pages/ledger_page.dart';
import 'package:vimbai_mobile_client/pages/trial_balance_page.dart';
import 'package:vimbai_mobile_client/pages/balance_sheet_page.dart';
import 'package:vimbai_mobile_client/pages/income_statement_page.dart';
import 'package:vimbai_mobile_client/pages/cash_flow_statement_page.dart';
import 'package:vimbai_mobile_client/pages/budgets_page.dart';

class HomePage extends StatefulWidget {
  const HomePage({super.key});

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  final AuthService _authService = AuthService();
  final AccountingApiService _accountingApiService = AccountingApiService(); // NEW
  final BookSyncService _bookSync = BookSyncService.instance;
  final ValueNotifier<int> _contextTicker = ValueNotifier<int>(0);
  ConnectivityResult _connectivityResult = ConnectivityResult.none;

  @override
  void initState() {
    super.initState();
    _hydrateBookContext();
    BookContext.instance.addListener(() => _contextTicker.value++);
    _checkConnectivity();
    Connectivity().onConnectivityChanged.listen((List<ConnectivityResult> results) {
      setState(() {
        _connectivityResult = results.isEmpty ? ConnectivityResult.none : results.last;
      });
      if (!results.contains(ConnectivityResult.none)) { // NEW: Attempt sync when online
        _syncOfflineData();
      }
    });
  }

  Future<void> _checkConnectivity() async {
    final results = await Connectivity().checkConnectivity();
    _connectivityResult = results.isEmpty ? ConnectivityResult.none : results.last;
    setState(() {});
  }

  Future<void> _syncOfflineData() async { // NEW
    if (_connectivityResult != ConnectivityResult.none) {
      try {
        await _accountingApiService.syncOfflineJournalEntries();
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('Offline Journal Entries synced successfully!')),
          );
        }
      } catch (e) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Error syncing offline data: ${e.toString()}')),
          );
        }
      }
    }
  }

  /// Hydrate the app-wide Book context from the persisted active book so
  /// service clients send X-Book-ID from the moment the app opens.
  Future<void> _hydrateBookContext() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final activeId = prefs.getString('active_book_id');
      if (activeId == null) return;
      List<VBook> books;
      try {
        books = await _bookSync.refreshBooksFromServer();
      } catch (_) {
        books = await _bookSync.localBooks();
      }
      for (final b in books) {
        if (b.id == activeId && b.membershipStatus == 'active') {
          BookContext.instance.setBook(BookContextBook(
            id: b.id,
            name: b.name,
            tier: b.tier,
            yourRole: b.yourRole,
            source: 'sync',
          ));
          return;
        }
      }
    } catch (_) {
      // offline or not logged in yet - BooksPage will hydrate later
    }
  }

  /// Builds a full-width navigation button that pushes [page].
  Widget _navButton(BuildContext context, String label, Widget page) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 10.0),
      child: ElevatedButton(
        onPressed: () {
          Navigator.of(context).push(
            MaterialPageRoute(builder: (context) => page),
          );
        },
        child: Text(label),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Vimbai Home (Offline Ready)'),
            actions: [
              IconButton(
                icon: const Icon(Icons.logout),
                onPressed: () async {
                  await _authService.logout();
                  if (mounted) {
                    Navigator.of(context).pushReplacement(
                      MaterialPageRoute(builder: (context) => const LoginPage()),
                    );
                  }
                },
              ),
              IconButton( // NEW: Manual Sync Button
                icon: const Icon(Icons.cloud_upload),
                onPressed: _connectivityResult == ConnectivityResult.none ? null : _syncOfflineData,
                tooltip: 'Sync Offline Data',
              ),
            ],
          ),
          body: Center(
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  // ------------------------------------------------------
                  // Active Book context - every service call below runs
                  // inside the selected Book (X-Book-ID via gateway).
                  // ------------------------------------------------------
                  ValueListenableBuilder<int>(
                    valueListenable: _contextTicker,
                    builder: (context, _, __) {
                      final b = BookContext.instance.current;
                      return Card(
                        color: b == null
                            ? null
                            : Theme.of(context).colorScheme.primaryContainer,
                        child: ListTile(
                          leading: Icon(
                            b == null ? Icons.person : Icons.book,
                          ),
                          title: Text(
                            b == null ? 'Personal context' : b.name,
                          ),
                          subtitle: Text(
                            b == null
                                ? 'tap to choose a Book context'
                                : 'your ${b.tier} Book - you are ${b.yourRole}',
                          ),
                          onTap: () {
                            Navigator.of(context).push(
                              MaterialPageRoute(
                                builder: (context) => const BooksPage(),
                              ),
                            );
                          },
                        ),
                      );
                    },
                  ),
                  const SizedBox(height: 16),

                  // ------------------------------------------------------
                  // Accounting core (wired to accounting-service via gateway)
                  // ------------------------------------------------------
                  const Text(
                    'Accounting:',
                    style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 10),
                  _navButton(context, 'Journal Entries', const JournalEntriesListPage()),
                  _navButton(context, 'Chart of Accounts', const ChartOfAccountsPage()),
                  _navButton(context, 'Ledger', const LedgerPage()),
                  _navButton(context, 'Trial Balance', const TrialBalancePage()),
                  const SizedBox(height: 20),

                  // ------------------------------------------------------
                  // Financial statements (wired to accounting-service)
                  // ------------------------------------------------------
                  const Text(
                    'Financial Statements:',
                    style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 10),
                  _navButton(context, 'Balance Sheet', const BalanceSheetPage()),
                  _navButton(context, 'Income Statement', const IncomeStatementPage()),
                  _navButton(context, 'Cash Flow Statement', const CashFlowStatementPage()),
                  const SizedBox(height: 20),

                  // ------------------------------------------------------
                  // Budgets (local-first CRUD + remote variance analysis)
                  // ------------------------------------------------------
                  const Text(
                    'Budgets:',
                    style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 10),
                  _navButton(context, 'Budgets', const BudgetsPage()),
                  const SizedBox(height: 20),

                  const Text(
                    'Multimodal Input:',
                    style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 10),
                  _navButton(context, 'Process Image/Audio', const MultimodalInputPage()),
                  const SizedBox(height: 10),
                  _navButton(context, 'Your Books', const BooksPage()),
                  const SizedBox(height: 10),
                  _navButton(context, 'Non-profit Organizations', const NpoPage()),
                  const SizedBox(height: 10),
                  _navButton(context, 'Personal finance', const PersonalFinancePage()),
                  const SizedBox(height: 20),
                  _navButton(context, 'View Financial Ratios', const FinancialRatiosPage()),
                  const SizedBox(height: 30),
                  const Text(
                    'Banking Integration:',
                    style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 10),
                  _navButton(context, 'Manage Bank Accounts', const BankAccountsPage()),
                ],
              ),
            ),
          ),
        );
      }
    }
