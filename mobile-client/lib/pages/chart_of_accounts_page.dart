import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/services/accounting_api_service.dart';

class ChartOfAccountsPage extends StatefulWidget {
  const ChartOfAccountsPage({super.key});

  @override
  State<ChartOfAccountsPage> createState() => _ChartOfAccountsPageState();
}

class _ChartOfAccountsPageState extends State<ChartOfAccountsPage> {
  late Future<List<Account>> _accountsFuture;
  final AccountingApiService _apiService = AccountingApiService();

  @override
  void initState() {
    super.initState();
    _accountsFuture = _apiService.getChartOfAccounts();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Chart of Accounts'),
      ),
      body: FutureBuilder<List<Account>>(
        future: _accountsFuture,
        builder: (context, snapshot) {
          if (snapshot.connectionState == ConnectionState.waiting) {
            return const Center(child: CircularProgressIndicator());
          } else if (snapshot.hasError) {
            return Center(child: Text('Error: ${snapshot.error}'));
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
