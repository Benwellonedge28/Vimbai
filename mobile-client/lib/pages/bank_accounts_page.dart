import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/banking_api_service.dart';
import 'package:vimbai_mobile_client/models/banking_models.dart';
import 'package:vimbai_mobile_client/pages/bank_account_detail_page.dart';

class BankAccountsPage extends StatefulWidget {
  const BankAccountsPage({super.key});

  @override
  State<BankAccountsPage> createState() => _BankAccountsPageState();
}

class _BankAccountsPageState extends State<BankAccountsPage> {
  late Future<List<BankConnection>> _connectionsFuture;
  final BankingApiService _apiService = BankingApiService();

  final _formKey = GlobalKey<FormState>();
  final TextEditingController _bankNameController = TextEditingController();
  final TextEditingController _accountNumberController = TextEditingController();
  final TextEditingController _apiKeyController = TextEditingController();
  String _accountType = 'checking'; // Default account type

  @override
  void initState() {
    super.initState();
    _connectionsFuture = _apiService.getConnections();
  }

  void _refreshConnections() {
    setState(() {
      _connectionsFuture = _apiService.getConnections();
    });
  }

  Future<void> _connectBank() async {
    if (_formKey.currentState!.validate()) {
      try {
        await _apiService.connectBank(
          bankName: _bankNameController.text.trim(),
          accountNumber: _accountNumberController.text.trim(),
          apiKey: _apiKeyController.text.trim(),
          accountType: _accountType,
        );
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('Bank account linked successfully!')),
          );
          Navigator.of(context).pop(); // Close dialog
          _refreshConnections();
        }
      } catch (e) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Error linking bank account: ${e.toString()}')),
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
          title: const Text('Link New Bank Account'),
          content: SingleChildScrollView(
            child: Form(
              key: _formKey,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  TextFormField(
                    controller: _bankNameController,
                    decoration: const InputDecoration(labelText: 'Bank Name'),
                    validator: (value) => value!.isEmpty ? 'Required' : null,
                  ),
                  TextFormField(
                    controller: _accountNumberController,
                    decoration: const InputDecoration(labelText: 'Account Number'),
                    validator: (value) => value!.isEmpty ? 'Required' : null,
                  ),
                  TextFormField(
                    controller: _apiKeyController,
                    decoration: const InputDecoration(labelText: 'Bank API Key'),
                    obscureText: true,
                    validator: (value) => value!.isEmpty ? 'Required' : null,
                  ),
                  DropdownButtonFormField<String>(
                    initialValue: _accountType,
                    decoration: const InputDecoration(labelText: 'Account Type'),
                    items: <String>['checking', 'savings', 'credit_card', 'loan'].map((String value) {
                      return DropdownMenuItem<String>(value: value, child: Text(value));
                    }).toList(),
                    onChanged: (String? newValue) {
                      if (newValue != null) {
                        setState(() { _accountType = newValue; });
                      }
                    },
                  ),
                ],
              ),
            ),
          ),
          actions: [
            TextButton(onPressed: () => Navigator.of(context).pop(), child: const Text('Cancel')),
            ElevatedButton(onPressed: _connectBank, child: const Text('Link Account')),
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
        content: Text('${connection.bankName} ${connection.accountNumber} will be unlinked.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Cancel')),
          TextButton(onPressed: () => Navigator.pop(context, true), child: const Text('Disconnect')),
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
          SnackBar(content: Text('Error disconnecting: ${e.toString()}')),
        );
      }
    }
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
          body: FutureBuilder<List<BankConnection>>(
            future: _connectionsFuture,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              } else if (snapshot.hasError) {
                return Center(child: Text('Error: ${snapshot.error}'));
              } else if (!snapshot.hasData || snapshot.data!.isEmpty) {
                return const Center(child: Text('No bank accounts linked.'));
              } else {
                return ListView.builder(
                  itemCount: snapshot.data!.length,
                  itemBuilder: (context, index) {
                    final connection = snapshot.data![index];
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
                          title: Text('${connection.bankName} - ${connection.accountNumber}'),
                          subtitle: Text(
                            '${connection.accountType.toUpperCase()} | ${connection.status.toUpperCase()}'
                            '${connection.lastSync != null ? ' | Synced: ${connection.lastSync!.toLocal().toString().substring(0, 16)}' : ''}',
                          ),
                          trailing: Icon(
                            connection.status == 'active' ? Icons.link : Icons.link_off,
                            color: connection.status == 'active' ? Colors.green : Colors.grey,
                          ),
                          onTap: () {
                            Navigator.of(context).push(MaterialPageRoute(
                              builder: (context) => BankAccountDetailPage(connection: connection),
                            ));
                          },
                        ),
                      ),
                    );
                  },
                );
              }
            },
          ),
          floatingActionButton: FloatingActionButton(
            onPressed: _showConnectDialog,
            child: const Icon(Icons.add),
          ),
        );
      }
    }
