import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/banking/data/banking_api_service.dart';
import 'package:vimbai_mobile_client/features/banking/models/banking_models.dart';
import 'package:intl/intl.dart';

class BankAccountDetailPage extends StatefulWidget {
  final BankConnection connection;

  /// The engine this screen talks to. A test hands in its own so the screen can be read without a phone.
  final BankingApiService? service;

  const BankAccountDetailPage({
    super.key,
    required this.connection,
    this.service,
  });

  @override
  State<BankAccountDetailPage> createState() => _BankAccountDetailPageState();
}

class _BankAccountDetailPageState extends State<BankAccountDetailPage> {
  late final BankingApiService _apiService =
      widget.service ?? BankingApiService();
  bool _isSyncing = false;

  List<BankTransaction> _transactions = const [];
  bool _loading = true;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _refreshTransactions();
  }

  Future<void> _syncTransactions() async {
    setState(() {
      _isSyncing = true;
    });
    try {
      await _apiService.syncTransactions(widget.connection.id);
      if (mounted) {
        // A hand-added account has no provider behind it, so saying "completed" would claim rows it never fetched.
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text(
              'A sync was asked for. This account was added by hand, so no bank sends rows here; '
              'they arrive when a statement is brought in.',
            ),
          ),
        );
      }
      _refreshTransactions();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              'The sync was not started: ${CouldNotRead.reasonOf(e)}',
            ),
          ),
        );
      }
    } finally {
      setState(() {
        _isSyncing = false;
      });
    }
  }

  /// Read this account's rows again, holding the result as state so a read that fails at once reaches the error branch.
  Future<void> _refreshTransactions() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final rows = await _apiService.getTransactions(widget.connection.id);
      if (mounted) setState(() => _transactions = rows);
    } catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  /// What this account's list shows when nothing has been brought in yet.
  Widget _noTransactionsYet() {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Text(
          'No rows from this account yet. Rows appear when a statement is brought in and matched with the entries '
          'you post; a hand-added account is not read from the bank by itself.',
          textAlign: TextAlign.center,
          style: Theme.of(context).textTheme.bodySmall,
        ),
      ),
    );
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
            TextButton(
              onPressed: () => Navigator.pop(context, null),
              child: const Text('Cancel'),
            ),
            ElevatedButton(
              onPressed: () => Navigator.pop(context, controller.text),
              child: const Text('Reconcile'),
            ),
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
        title: Text(widget.connection.label),
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
            Text(
              widget.connection.accountName,
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
            ),
            if (widget.connection.last4.isNotEmpty)
              Text('Account ending ${widget.connection.last4}'),
            Text('Type: ${widget.connection.accountType.toUpperCase()}'),
            Text('Status: ${widget.connection.status.toUpperCase()}'),
            Text(
              'Last Synced: ${widget.connection.lastSync != null ? DateFormat.yMd().add_jm().format(widget.connection.lastSync!) : 'Never'}',
            ),
            const SizedBox(height: 20),
            const Text(
              'Transactions:',
              style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
            ),
            const SizedBox(height: 10),
            Expanded(child: _rows()),
          ],
        ),
      ),
    );
  }

  /// The rows, or the reason there are none to show.
  Widget _rows() {
    if (_loading || _isSyncing) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_error != null) {
      return CouldNotRead(
        what: 'The rows from this account',
        error: _error,
        onRetry: _refreshTransactions,
      );
    }
    if (_transactions.isEmpty) return _noTransactionsYet();
    return ListView.builder(
      itemCount: _transactions.length,
      itemBuilder: (context, index) {
        final transaction = _transactions[index];
        final isCredit = transaction.transactionType == 'credit';
        return Card(
          margin: const EdgeInsets.symmetric(vertical: 4),
          child: ListTile(
            title: Text(
              transaction.description.isEmpty
                  ? (transaction.reference.isEmpty
                        ? 'Transaction'
                        : transaction.reference)
                  : transaction.description,
            ),
            subtitle: Text(
              '${DateFormat.yMd().format(transaction.transactionDate)}'
              '${transaction.reconciled ? ' | Reconciled' : ' | Unreconciled'}',
            ),
            trailing: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  // Every digit, in full: a large posting is not rounded to two places on the way out.
                  '${isCredit ? '+' : '-'}${transaction.amount}',
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
}
