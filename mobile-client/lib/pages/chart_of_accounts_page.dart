import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/accounting_api_service.dart';
import 'package:vimbai_mobile_client/models/accounting_models.dart';
import 'package:connectivity_plus/connectivity_plus.dart';

class ChartOfAccountsPage extends StatefulWidget {
  const ChartOfAccountsPage({super.key});

  @override
  State<ChartOfAccountsPage> createState() => _ChartOfAccountsPageState();
}

class _ChartOfAccountsPageState extends State<ChartOfAccountsPage> {
  late Future<List<Account>> _accountsFuture;
  final AccountingApiService _apiService = AccountingApiService();
  ConnectivityResult _connectivityResult = ConnectivityResult.none;

  @override
  void initState() {
    super.initState();
    _checkConnectivity();
    Connectivity().onConnectivityChanged.listen((List<ConnectivityResult> results) {
      setState(() {
        _connectivityResult = results.isEmpty ? ConnectivityResult.none : results.last;
        _loadAccounts();
      });
    });
    _loadAccounts();
  }

  Future<void> _checkConnectivity() async {
    final results = await Connectivity().checkConnectivity();
    _connectivityResult = results.isEmpty ? ConnectivityResult.none : results.last;
    setState(() {});
  }

  Future<void> _loadAccounts({bool forceRemote = false}) async {
    setState(() {
      _accountsFuture = _apiService.getAllAccounts();
    });
  }

  IconData _getAccountTypeIcon(String accountType) {
    switch (accountType.toLowerCase()) {
      case 'asset':
        return Icons.account_balance_wallet;
      case 'liability':
        return Icons.credit_card;
      case 'equity':
        return Icons.pie_chart;
      case 'revenue':
      case 'income':
        return Icons.trending_up;
      case 'expense':
        return Icons.trending_down;
      default:
        return Icons.account_balance;
    }
  }

  Color _getAccountTypeColor(String accountType) {
    switch (accountType.toLowerCase()) {
      case 'asset':
        return Colors.blue;
      case 'liability':
        return Colors.orange;
      case 'equity':
        return Colors.purple;
      case 'revenue':
      case 'income':
        return Colors.green;
      case 'expense':
        return Colors.red;
      default:
        return Colors.grey;
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Chart of Accounts'),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: () => _loadAccounts(forceRemote: true),
            tooltip: 'Refresh from Server',
          ),
        ],
      ),
      body: Column(
        children: [
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
            color: _connectivityResult == ConnectivityResult.none
                ? Colors.orange.shade100
                : Colors.green.shade100,
            child: Row(
              children: [
                Icon(
                  _connectivityResult == ConnectivityResult.none
                      ? Icons.cloud_off
                      : Icons.cloud_done,
                  size: 16,
                  color: _connectivityResult == ConnectivityResult.none
                      ? Colors.orange
                      : Colors.green,
                ),
                const SizedBox(width: 8),
                Text(
                  _connectivityResult == ConnectivityResult.none
                      ? 'Offline Mode - Showing cached data'
                      : 'Online - Connected to server',
                  style: TextStyle(
                    fontSize: 12,
                    color: _connectivityResult == ConnectivityResult.none
                        ? Colors.orange.shade800
                        : Colors.green.shade800,
                  ),
                ),
              ],
            ),
          ),
          Expanded(
            child: FutureBuilder<List<Account>>(
              future: _accountsFuture,
              builder: (context, snapshot) {
                if (snapshot.connectionState == ConnectionState.waiting) {
                  return const Center(child: CircularProgressIndicator());
                } else if (snapshot.hasError) {
                  return Center(
                    child: Padding(
                      padding: const EdgeInsets.all(16.0),
                      child: Column(
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          const Icon(Icons.error_outline, size: 48, color: Colors.red),
                          const SizedBox(height: 16),
                          Text(
                            'Error: ${snapshot.error}',
                            textAlign: TextAlign.center,
                          ),
                          if (_connectivityResult == ConnectivityResult.none)
                            const Padding(
                              padding: EdgeInsets.only(top: 8),
                              child: Text('Showing cached data while offline.'),
                            ),
                        ],
                      ),
                    ),
                  );
                } else if (!snapshot.hasData || snapshot.data!.isEmpty) {
                  return const Center(child: Text('No accounts found.'));
                } else {
                  final accounts = snapshot.data!;
                  final groupedAccounts = <String, List<Account>>{};

                  for (var account in accounts) {
                    final type = account.accountType;
                    groupedAccounts.putIfAbsent(type, () => []);
                    groupedAccounts[type]!.add(account);
                  }

                  return ListView(
                    children: groupedAccounts.entries.map((entry) {
                      return Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Container(
                            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                            color: _getAccountTypeColor(entry.key).withValues(alpha: 0.1),
                            child: Row(
                              children: [
                                Icon(
                                  _getAccountTypeIcon(entry.key),
                                  color: _getAccountTypeColor(entry.key),
                                  size: 20,
                                ),
                                const SizedBox(width: 8),
                                Text(
                                  entry.key.toUpperCase(),
                                  style: TextStyle(
                                    fontWeight: FontWeight.bold,
                                    color: _getAccountTypeColor(entry.key),
                                  ),
                                ),
                                const Spacer(),
                                Text(
                                  '${entry.value.length} accounts',
                                  style: TextStyle(
                                    fontSize: 12,
                                    color: _getAccountTypeColor(entry.key).withValues(alpha: 0.7),
                                  ),
                                ),
                              ],
                            ),
                          ),
                          ...entry.value.map((account) => ListTile(
                            leading: CircleAvatar(
                              backgroundColor: _getAccountTypeColor(entry.key).withValues(alpha: 0.2),
                              child: Icon(
                                _getAccountTypeIcon(entry.key),
                                color: _getAccountTypeColor(entry.key),
                                size: 20,
                              ),
                            ),
                            title: Text(account.accountName),
                            subtitle: Row(
                              children: [
                                Container(
                                  padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                                  decoration: BoxDecoration(
                                    color: Colors.grey.shade200,
                                    borderRadius: BorderRadius.circular(4),
                                  ),
                                  child: Text(
                                    account.accountNumber,
                                    style: const TextStyle(fontSize: 12, fontWeight: FontWeight.bold),
                                  ),
                                ),
                                const SizedBox(width: 8),
                                Text(
                                  'Normal: ${account.normalBalance}',
                                  style: const TextStyle(fontSize: 12, color: Colors.grey),
                                ),
                              ],
                            ),
                            trailing: account.description != null
                                ? Tooltip(
                                    message: account.description!,
                                    child: const Icon(Icons.info_outline, size: 20),
                                  )
                                : null,
                            onTap: () {
                              // Could navigate to account detail page
                            },
                          )),
                          const Divider(),
                        ],
                      );
                    }).toList(),
                  );
                }
              },
            ),
          ),
        ],
      ),
    );
  }
}