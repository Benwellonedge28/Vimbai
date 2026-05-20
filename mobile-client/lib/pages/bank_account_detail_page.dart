import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/services/banking_api_service.dart';
import 'package:finacc_mobile_client/models/banking_models.dart';
import 'package:intl/intl.dart';

class BankAccountDetailPage extends StatefulWidget {
  final BankAccount bankAccount;
  const BankAccountDetailPage({super.key, required this.bankAccount});

  @override
  State<BankAccountDetailPage> createState() => _BankAccountDetailPageState();
}

class _BankAccountDetailPageState extends State<BankAccountDetailPage> {
  late Future<List<BankTransaction>> _transactionsFuture;
  final BankingApiService _apiService = BankingApiService();
  bool _isFetchingTransactions = false;

  @override
  void initState() {
    super.initState();
    _transactionsFuture = _apiService.getTransactionsForAccount(widget.bankAccount.accountId);
  }

  Future<void> _fetchNewTransactions() async {
    setState(() {
      _isFetchingTransactions = true;
    });
    try {
      // This calls the mocked endpoint in the backend
      await _apiService.fetchAndStoreTransactions(widget.bankAccount.accountId);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('New transactions fetched and stored!')),
        );
      }
      _refreshTransactions();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Error fetching transactions: ${e.toString()}')),
        );
      }
    } finally {
      setState(() {
        _isFetchingTransactions = false;
      });
    }
  }

  void _refreshTransactions() {
    setState(() {
      _transactionsFuture = _apiService.getTransactionsForAccount(widget.bankAccount.accountId);
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: Text(widget.bankAccount.accountName),
            actions: [
              IconButton(
                icon: const Icon(Icons.sync),
                onPressed: _isFetchingTransactions ? null : _fetchNewTransactions,
                tooltip: 'Fetch New Transactions',
              ),
              IconButton(
                icon: const Icon(Icons.refresh),
                onPressed: _refreshTransactions,
                tooltip: 'Refresh Transactions',
              ),
            ],
          ),
          body: Padding(
            padding: const EdgeInsets.all(16.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Bank: ${widget.bankAccount.bankName}', style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                Text('Account ID: ${widget.bankAccount.accountId}'),
                Text('Type: ${widget.bankAccount.accountType.toUpperCase()}'),
                Text('Current Balance: ${widget.bankAccount.currency} ${widget.bankAccount.currentBalance.toStringAsFixed(2)}'),
                Text('Last Synced: ${widget.bankAccount.lastSyncedAt != null ? DateFormat.yMd().add_jm().format(widget.bankAccount.lastSyncedAt!) : 'Never'}'),
                const SizedBox(height: 20),
                const Text('Transactions:', style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
                const SizedBox(height: 10),
                Expanded(
                  child: FutureBuilder<List<BankTransaction>>(
                    future: _transactionsFuture,
                    builder: (context, snapshot) {
                      if (snapshot.connectionState == ConnectionState.waiting || _isFetchingTransactions) {
                        return const Center(child: CircularProgressIndicator());
                      } else if (snapshot.hasError) {
                        return Center(child: Text('Error: ${snapshot.error}'));
                      } else if (!snapshot.hasData || snapshot.data!.isEmpty) {
                        return const Center(child: Text('No transactions found for this account.'));
                      } else {
                        return ListView.builder(
                          itemCount: snapshot.data!.length,
                          itemBuilder: (context, index) {
                            final transaction = snapshot.data![index];
                            return Card(
                              margin: const EdgeInsets.symmetric(vertical: 4),
                              child: ListTile(
                                title: Text(transaction.description),
                                subtitle: Text(DateFormat.yMd().format(transaction.date)),
                                trailing: Text(
                                  '${transaction.amount.toStringAsFixed(2)} ${widget.bankAccount.currency}',
                                  style: TextStyle(
                                    color: transaction.amount > 0 ? Colors.green : Colors.red,
                                    fontWeight: FontWeight.bold,
                                  ),
                                ),
                              ),
                            );
                          },
                        );
                      }
                    },
                  ),
                ),
              ],
            ),
          ),
        );
      }
    }
