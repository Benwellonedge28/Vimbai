import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/banking_api_service.dart';
import 'package:vimbai_mobile_client/models/banking_models.dart';
import 'package:intl/intl.dart';

class BankAccountDetailPage extends StatefulWidget {
  final BankConnection connection;
  const BankAccountDetailPage({super.key, required this.connection});

  @override
  State<BankAccountDetailPage> createState() => _BankAccountDetailPageState();
}

class _BankAccountDetailPageState extends State<BankAccountDetailPage> {
  late Future<List<BankTransaction>> _transactionsFuture;
  final BankingApiService _apiService = BankingApiService();
  bool _isSyncing = false;

  @override
  void initState() {
    super.initState();
    _transactionsFuture = _apiService.getTransactions(widget.connection.id);
  }

  Future<void> _syncTransactions() async {
    setState(() {
      _isSyncing = true;
    });
    try {
      await _apiService.syncTransactions(widget.connection.id);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Transaction sync completed.')),
        );
      }
      _refreshTransactions();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Error syncing transactions: ${e.toString()}')),
        );
      }
    } finally {
      setState(() {
        _isSyncing = false;
      });
    }
  }

  void _refreshTransactions() {
    setState(() {
      _transactionsFuture = _apiService.getTransactions(widget.connection.id);
    });
  }

  Future<void> _reconcile(BankTransaction transaction) async {
    final notes = await showDialog<String>(
      context: context,
      builder: (context) {
        final controller = TextEditingController();
        return AlertDialog(
          title: const Text('Reconcile transaction'),
          content: TextField(
            controller: controller,
            decoration: const InputDecoration(
              labelText: 'Notes (optional)',
              hintText: 'e.g. matched against journal entry',
            ),
          ),
          actions: [
            TextButton(onPressed: () => Navigator.pop(context, null), child: const Text('Cancel')),
            ElevatedButton(onPressed: () => Navigator.pop(context, controller.text), child: const Text('Reconcile')),
          ],
        );
      },
    );
    if (notes == null) return;
    try {
      await _apiService.reconcileTransaction(
        widget.connection.id,
        transaction.id,
        notes: notes,
      );
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Transaction reconciled.')),
        );
      }
      _refreshTransactions();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Error reconciling: ${e.toString()}')),
        );
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: Text('${widget.connection.bankName} ${widget.connection.accountNumber}'),
            actions: [
              IconButton(
                icon: const Icon(Icons.sync),
                onPressed: _isSyncing ? null : _syncTransactions,
                tooltip: 'Sync Transactions',
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
                Text('Bank: ${widget.connection.bankName}', style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                Text('Account Number: ${widget.connection.accountNumber}'),
                Text('Type: ${widget.connection.accountType.toUpperCase()}'),
                Text('Status: ${widget.connection.status.toUpperCase()}'),
                Text('Last Synced: ${widget.connection.lastSync != null ? DateFormat.yMd().add_jm().format(widget.connection.lastSync!) : 'Never'}'),
                const SizedBox(height: 20),
                const Text('Transactions:', style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
                const SizedBox(height: 10),
                Expanded(
                  child: FutureBuilder<List<BankTransaction>>(
                    future: _transactionsFuture,
                    builder: (context, snapshot) {
                      if (snapshot.connectionState == ConnectionState.waiting || _isSyncing) {
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
                            final isCredit = transaction.transactionType == 'credit';
                            return Card(
                              margin: const EdgeInsets.symmetric(vertical: 4),
                              child: ListTile(
                                title: Text(transaction.description.isEmpty
                                    ? (transaction.reference.isEmpty ? 'Transaction' : transaction.reference)
                                    : transaction.description),
                                subtitle: Text(
                                  '${DateFormat.yMd().format(transaction.transactionDate)}'
                                  '${transaction.reconciled ? ' | Reconciled' : ' | Unreconciled'}',
                                ),
                                trailing: Row(
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    Text(
                                      '${isCredit ? '+' : '-'}${transaction.amount.toStringAsFixed(2)}',
                                      style: TextStyle(
                                        color: isCredit ? Colors.green : Colors.red,
                                        fontWeight: FontWeight.bold,
                                      ),
                                    ),
                                    if (!transaction.reconciled)
                                      IconButton(
                                        icon: const Icon(Icons.check_circle_outline),
                                        tooltip: 'Reconcile',
                                        onPressed: () => _reconcile(transaction),
                                      ),
                                  ],
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
