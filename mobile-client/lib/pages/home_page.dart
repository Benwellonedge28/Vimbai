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
import 'package:vimbai_mobile_client/pages/create_book_wizard.dart';
import 'package:vimbai_mobile_client/widgets/book_settings_sheet.dart';
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

  // Your Books on the home screen: searchable, sortable list.
  List<VBook> _books = [];
  String _searchQuery = '';
  String _sortMode = 'nameAz';
  static const Map<String, String> _sortLabels = {
    'nameAz': 'Name A-Z',
    'nameZa': 'Name Z-A',
    'newest': 'Newest first',
    'oldest': 'Oldest first',
    'category': 'Category',
    'folder': 'Folder',
  };
  static const Map<String, IconData> _tierIcons = {
    'personal': Icons.person,
    'household': Icons.home,
    'group': Icons.groups,
    'business': Icons.business_center,
    'nonprofit': Icons.volunteer_activism,
  };

  @override
  void initState() {
    super.initState();
    _hydrateBookContext();
    _loadSortMode();
    _loadBooks();
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
  Future<void> _loadBooks() async {
    List<VBook> books;
    try {
      books = await _bookSync.refreshBooksFromServer();
    } catch (_) {
      books = await _bookSync.localBooks();
    }
    if (mounted) setState(() => _books = books);
  }

  Future<void> _loadSortMode() async {
    final prefs = await SharedPreferences.getInstance();
    final mode = prefs.getString('home_book_sort');
    if (mode != null && _sortLabels.containsKey(mode) && mounted) {
      setState(() => _sortMode = mode);
    }
  }

  Future<void> _setSortMode(String mode) async {
    setState(() => _sortMode = mode);
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString('home_book_sort', mode);
  }

  /// Search + sort the Books list for the home screen.
  List<VBook> _filteredBooks() {
    final q = _searchQuery.trim().toLowerCase();
    final list = _books.where((b) {
      if (q.isEmpty) return true;
      return b.name.toLowerCase().contains(q) ||
          b.tier.toLowerCase().contains(q) ||
          b.folder.toLowerCase().contains(q);
    }).toList();
    switch (_sortMode) {
      case 'nameZa':
        list.sort((a, b) => b.name.compareTo(a.name));
      case 'newest':
        list.sort((a, b) => (b.createdAt?.millisecondsSinceEpoch ?? 0)
            .compareTo(a.createdAt?.millisecondsSinceEpoch ?? 0));
      case 'oldest':
        list.sort((a, b) => (a.createdAt?.millisecondsSinceEpoch ?? 0)
            .compareTo(b.createdAt?.millisecondsSinceEpoch ?? 0));
      case 'category':
        list.sort((a, b) {
          final c = a.tier.compareTo(b.tier);
          return c != 0 ? c : a.name.compareTo(b.name);
        });
      case 'folder':
        list.sort((a, b) {
          final c = a.folder.compareTo(b.folder);
          return c != 0 ? c : a.name.compareTo(b.name);
        });
      default:
        list.sort((a, b) => a.name.compareTo(b.name));
    }
    return list;
  }

  /// Make the tapped Book the active context for every service call.
  Future<void> _activateBook(VBook b) async {
    if (b.membershipStatus == 'invited') {
      // invitations are accepted on the Books page
      await _openBooksPage();
      return;
    }
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString('active_book_id', b.id);
    await _hydrateBookContext();
    await _loadBooks();
  }

  Future<void> _openBooksPage() async {
    await Navigator.of(context).push(
      MaterialPageRoute(builder: (context) => const BooksPage()),
    );
    await _loadBooks();
  }

  /// Per-Book settings (rename, folder, members, sync) - the kebab menu
  /// on each Book card. Applies to that Book only.
  void _openBookSettings(VBook b) {
    final folders = _books
        .map((x) => x.folder)
        .where((f) => f.isNotEmpty)
        .toSet()
        .toList();
    showBookSettingsSheet(context, b, folders, onChanged: _loadBooks);
  }

  Future<void> _startNewBook() async {
    final created = await Navigator.of(context).push<String>(
      MaterialPageRoute(builder: (context) => const CreateBookWizardPage()),
    );
    if (created is String && created.isNotEmpty) {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString('active_book_id', created);
      await _hydrateBookContext();
    }
    await _loadBooks();
  }

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
          // Floating "create a Book" button - bottom-right, extended so it
          // is immediately visible when the app opens.
          floatingActionButton: FloatingActionButton.extended(
            onPressed: _startNewBook,
            icon: const Icon(Icons.add),
            label: const Text('Start a Book'),
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
                  // Your Books - searchable, sortable list of every Book the
                  // user created. Tapping one makes it the active context.
                  // ------------------------------------------------------
                  Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Expanded(
                            child: Text(
                              'YOUR BOOKS',
                              style: Theme.of(context).textTheme.titleMedium,
                            ),
                          ),
                          IconButton(
                            icon: const Icon(Icons.add),
                            tooltip: 'Start a Book',
                            onPressed: _startNewBook,
                          ),
                          PopupMenuButton<String>(
                            icon: const Icon(Icons.sort),
                            tooltip: 'Sort books',
                            onSelected: _setSortMode,
                            itemBuilder: (ctx) => _sortLabels.entries
                                .map(
                                  (e) => PopupMenuItem(
                                    value: e.key,
                                    child: Row(
                                      children: [
                                        if (e.key == _sortMode)
                                          const Icon(Icons.check, size: 18)
                                        else
                                          const SizedBox(width: 18),
                                        const SizedBox(width: 8),
                                        Text(e.value),
                                      ],
                                    ),
                                  ),
                                )
                                .toList(),
                          ),
                        ],
                      ),
                      TextField(
                        onChanged: (v) => setState(() => _searchQuery = v),
                        decoration: InputDecoration(
                          hintText: 'Search books by name, folder or category',
                          prefixIcon: const Icon(Icons.search),
                          isDense: true,
                          border: OutlineInputBorder(
                            borderRadius: BorderRadius.circular(12),
                          ),
                        ),
                      ),
                      const SizedBox(height: 8),
                      if (_books.isEmpty)
                        Padding(
                          padding: const EdgeInsets.symmetric(vertical: 12),
                          child: Text(
                            _searchQuery.trim().isEmpty
                                ? 'No books yet - start your first one'
                                : 'No books match your search',
                            style: Theme.of(context).textTheme.bodySmall,
                          ),
                        )
                      else ..._filteredBooks().map(
                        (b) {
                          final active = b.id ==
                              BookContext.instance.current?.id;
                          final invited = b.membershipStatus == 'invited';
                          return Card(
                            margin: const EdgeInsets.symmetric(vertical: 4),
                            color: active
                                ? Theme.of(context)
                                    .colorScheme
                                    .primaryContainer
                                : null,
                            child: ListTile(
                              dense: true,
                              leading: Icon(
                                _tierIcons[b.tier] ?? Icons.book,
                              ),
                              title: Text(b.name),
                              subtitle: Text(
                                b.folder.isEmpty
                                    ? b.tier
                                    : '${b.folder} - ${b.tier}',
                              ),
                              trailing: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  if (invited)
                                    const Chip(label: Text('invited'))
                                  else if (active)
                                    const Icon(Icons.check_circle),
                                  IconButton(
                                    icon: const Icon(Icons.more_vert),
                                    tooltip: 'Book settings',
                                    onPressed: () =>
                                        _openBookSettings(b),
                                  ),
                                ],
                              ),
                              onTap: () => _activateBook(b),
                              onLongPress: _openBooksPage,
                            ),
                          );
                        },
                      ),
                      TextButton(
                        onPressed: _openBooksPage,
                        child: const Text('Manage books, folders and members'),
                      ),
                    ],
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
