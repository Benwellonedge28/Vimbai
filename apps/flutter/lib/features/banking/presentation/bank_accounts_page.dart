import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/banking/data/banking_api_service.dart';
import 'package:vimbai_mobile_client/features/banking/models/banking_models.dart';
import 'package:vimbai_mobile_client/features/banking/presentation/bank_account_detail_page.dart';

class BankAccountsPage extends StatefulWidget {
  const BankAccountsPage({super.key, this.service});

  /// The engine this screen talks to. A test hands in its own so the screen can be read without a phone.
  final BankingApiService? service;

  @override
  State<BankAccountsPage> createState() => _BankAccountsPageState();
}

class _BankAccountsPageState extends State<BankAccountsPage> {
  late final BankingApiService _apiService =
      widget.service ?? BankingApiService();

  final _formKey = GlobalKey<FormState>();
  final TextEditingController _accountNameController = TextEditingController();
  final TextEditingController _last4Controller = TextEditingController();
  String _accountType = 'checking'; // Default account type

  List<BankConnection> _connections = const [];
  bool _loading = true;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _refreshConnections();
  }

  /// Read the list again, holding the result as state so a read that fails at once still reaches the error branch.
  Future<void> _refreshConnections() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final connections = await _apiService.getConnections();
      if (mounted) setState(() => _connections = connections);
    } catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _connectBank() async {
    if (_formKey.currentState!.validate()) {
      try {
        await _apiService.connectBank(
          accountName: _accountNameController.text.trim(),
          last4: _last4Controller.text.trim(),
          accountType: _accountType,
        );
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('Bank account added to this Book.')),
          );
          Navigator.of(context).pop(); // Close dialog
          _refreshConnections();
        }
      } catch (e) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(
              content: Text(
                'The bank account was not added: ${CouldNotRead.reasonOf(e)}',
              ),
            ),
          );
        }
      }
    }
  }

  void _showConnectDialog() {
    showDialog(
      context: context,
      builder: (BuildContext context) {
        return AlertDialog(
          title: const Text('Add a bank account'),
          content: SingleChildScrollView(
            child: Form(
              key: _formKey,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Padding(
                    padding: EdgeInsets.only(bottom: 12),
                    child: Text(
                      'Name the account as you would on a statement, for example "CBZ current account". Its statement '
                      'can then be kept alongside the entries you post. No number longer than its last four digits is '
                      'ever stored.',
                    ),
                  ),
                  TextFormField(
                    controller: _accountNameController,
                    decoration: const InputDecoration(
                      labelText: 'Account name',
                      hintText: 'CBZ current account',
                    ),
                    validator: (value) =>
                        (value == null || value.trim().isEmpty)
                        ? 'Name the account'
                        : null,
                  ),
                  TextFormField(
                    controller: _last4Controller,
                    decoration: const InputDecoration(
                      labelText: 'Last four digits (optional)',
                    ),
                    maxLength: 4,
                  ),
                  DropdownButtonFormField<String>(
                    initialValue: _accountType,
                    decoration: const InputDecoration(
                      labelText: 'Account Type',
                    ),
                    items:
                        <String>[
                          'checking',
                          'savings',
                          'credit_card',
                          'loan',
                        ].map((String value) {
                          return DropdownMenuItem<String>(
                            value: value,
                            child: Text(value),
                          );
                        }).toList(),
                    onChanged: (String? newValue) {
                      if (newValue != null) {
                        setState(() {
                          _accountType = newValue;
                        });
                      }
                    },
                  ),
                ],
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('Cancel'),
            ),
            ElevatedButton(
              onPressed: _connectBank,
              child: const Text('Add it to this Book'),
            ),
          ],
        );
      },
    );
  }

  Future<void> _disconnect(BankConnection connection) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Disconnect bank account?'),
        content: Text(
          '${connection.label} will be taken off this Book. Its entries are not touched.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Disconnect'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      await _apiService.disconnect(connection.id);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Bank account disconnected.')),
        );
      }
      _refreshConnections();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              'The account was not taken off: ${CouldNotRead.reasonOf(e)}',
            ),
          ),
        );
      }
    }
  }

  /// The list, or the reason there is no list. A failed read is never shown as "no accounts".
  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'Your bank accounts',
        error: _error,
        onRetry: _refreshConnections,
      );
    }
    if (_connections.isEmpty) return _noAccountsYet();
    return ListView.builder(
      itemCount: _connections.length,
      itemBuilder: (context, index) {
        final connection = _connections[index];
        return Dismissible(
          key: Key(connection.id),
          direction: DismissDirection.endToStart,
          background: Container(
            color: Colors.red,
            alignment: Alignment.centerRight,
            padding: const EdgeInsets.only(right: 16),
            child: const Icon(Icons.link_off, color: Colors.white),
          ),
          confirmDismiss: (_) async {
            await _disconnect(connection);
            return false;
          },
          child: Card(
            margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
            child: ListTile(
              title: Text(connection.label),
              subtitle: Text(
                '${connection.accountType.toUpperCase()} | ${connection.status.toUpperCase()}'
                '${connection.lastSync != null ? ' | Synced: ${connection.lastSync!.toLocal().toString().substring(0, 16)}' : ''}',
              ),
              trailing: Icon(
                connection.status == 'active' ? Icons.link : Icons.link_off,
                color: connection.status == 'active'
                    ? Colors.green
                    : Colors.grey,
              ),
              onTap: () {
                Navigator.of(context).push(
                  MaterialPageRoute(
                    builder: (context) =>
                        BankAccountDetailPage(connection: connection),
                  ),
                );
              },
            ),
          ),
        );
      },
    );
  }

  /// No live bank feed is part of this install, so the empty screen says what this list is for and what it is not.
  Widget _noAccountsYet() {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text(
              'No bank accounts yet',
              style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
            ),
            const SizedBox(height: 8),
            Text(
              'This keeps a named bank account beside a Book, so its statement can be matched with the entries you '
              'post. Add one by name; nothing here logs in to your bank, and only the last four digits of a number '
              'are ever kept.',
              textAlign: TextAlign.center,
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 16),
            ElevatedButton.icon(
              onPressed: _showConnectDialog,
              icon: const Icon(Icons.add),
              label: const Text('Add the first account'),
            ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Bank Accounts'),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: _refreshConnections,
          ),
        ],
      ),
      body: _body(),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _showConnectDialog,
        icon: const Icon(Icons.add),
        label: const Text('Add account'),
      ),
    );
  }
}
