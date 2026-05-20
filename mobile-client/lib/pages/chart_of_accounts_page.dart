import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/services/accounting_api_service.dart';
import 'package:finacc_mobile_client/models/accounting_models.dart';
import 'package:connectivity_plus/connectivity_plus.dart'; // NEW

class ChartOfAccountsPage extends StatefulWidget {
  const ChartOfAccountsPage({super.key});

  @override
  State<ChartOfAccountsPage> createState() => _ChartOfAccountsPageState();
}

class _ChartOfAccountsPageState extends State<ChartOfAccountsPage> {
  late Future<List<Account>> _accountsFuture;
  final AccountingApiService _apiService = AccountingApiService();
  ConnectivityResult _connectivityResult = ConnectivityResult.none; // NEW

  @override
  void initState() {
    super.initState();
    _checkConnectivity(); // NEW
    Connectivity().onConnectivityChanged.listen((ConnectivityResult result) { // NEW
      setState(() {
        _connectivityResult = result;
        _loadAccounts(); // Reload accounts on connectivity change
      });
    });
    _loadAccounts();
  }

  Future<void> _checkConnectivity() async { // NEW
    _connectivityResult = await (Connectivity().checkConnectivity());
    setState(() {});
  }

  Future<void> _loadAccounts({bool forceRemote = false}) async { // NEW
    setState(() {
      _accountsFuture = _apiService.getChartOfAccounts(forceRemote: forceRemote);
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Chart of Accounts'),
            actions: [ // NEW: Refresh button to force remote load
              IconButton(
                icon: const Icon(Icons.refresh),
                onPressed: () => _loadAccounts(forceRemote: true),
                tooltip: 'Refresh from Server',
              ),
            ],
          ),
          body: FutureBuilder<List<Account>>(
            future: _accountsFuture,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              } else if (snapshot.hasError) {
                return Center(child: Text('Error: ${snapshot.error}\n${_connectivityResult == ConnectivityResult.none ? 'Showing local data.' : ''}')); // NEW error message
              } else if (!snapshot.hasData || snapshot.data!.isEmpty) {
                return const Center(child: Text('No accounts found.'));
              } else {
                return ListView.builder(
                  itemCount: snapshot.data!.length,
                  itemBuilder: (context, index) {
                    final account = snapshot.data![index];
                    return Card(
                      margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                      child: ListTile(
                        title: Text('${account.accountNumber} - ${account.accountName}'),
                        subtitle: Text('${account.accountType} (${account.normalBalance})'),
                        trailing: account.parentAccountNumber != null
                            ? Text('Parent: ${account.parentAccountNumber}')
                            : null,
                      ),
                    );
                  },
                );
              }
            },
          ),
        );
      }
    }
