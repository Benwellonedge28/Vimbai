import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/banking_api_service.dart';
import 'package:vimbai_mobile_client/models/banking_models.dart';
import 'package:vimbai_mobile_client/local_db/user_local_data.dart'; // To get current user ID
import 'package:vimbai_mobile_client/pages/bank_account_detail_page.dart';

class BankAccountsPage extends StatefulWidget {
  const BankAccountsPage({super.key});

  @override
  State<BankAccountsPage> createState() => _BankAccountsPageState();
}

class _BankAccountsPageState extends State<BankAccountsPage> {
  late Future<List<BankAccount>> _bankAccountsFuture;
  final BankingApiService _apiService = BankingApiService();

  final _formKey = GlobalKey<FormState>();
  final TextEditingController _bankNameController = TextEditingController();
  final TextEditingController _accountNameController = TextEditingController();
  final TextEditingController _accountIdController = TextEditingController();
  final TextEditingController _currencyController = TextEditingController(text: 'USD');
  final TextEditingController _initialBalanceController = TextEditingController();
  String _accountType = 'checking'; // Default account type

  @override
  void initState() {
    super.initState();
    _bankAccountsFuture = _apiService.getBankAccounts();
  }

  void _refreshBankAccounts() {
    setState(() {
      _bankAccountsFuture = _apiService.getBankAccounts();
    });
  }

  Future<void> _createBankAccount() async {
    if (_formKey.currentState!.validate()) {
      final userId = await UserLocalData.getUserId(); // Assume user ID is available locally
      if (userId == null) {
        if (mounted) ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('User not logged in locally.')));
        return;
      }

      final newAccount = BankAccount(
        userId: userId, // Backend will use actual JWT user_id, this is just for model.
        bankName: _bankNameController.text,
        accountName: _accountNameController.text,
        accountId: _accountIdController.text,
        accountType: _accountType,
        currency: _currencyController.text,
        currentBalance: double.parse(_initialBalanceController.text),
        isSynced: false,
        createdAt: DateTime.now(),
        updatedAt: DateTime.now(),
      );

      try {
        await _apiService.createBankAccount(newAccount);
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('Bank account linked successfully!')),
          );
          Navigator.of(context).pop(); // Close dialog
          _refreshBankAccounts();
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

  void _showCreateAccountDialog() {
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
                    controller: _accountNameController,
                    decoration: const InputDecoration(labelText: 'Account Name (e.g., Main Checking)'),
                    validator: (value) => value!.isEmpty ? 'Required' : null,
                  ),
                  TextFormField(
                    controller: _accountIdController,
                    decoration: const InputDecoration(labelText: 'Bank Account ID (Unique Identifier)'),
                    validator: (value) => value!.isEmpty ? 'Required' : null,
                  ),
                  DropdownButtonFormField<String>(
                    value: _accountType,
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
                  TextFormField(
                    controller: _currencyController,
                    decoration: const InputDecoration(labelText: 'Currency (e.g., USD)'),
                    validator: (value) => value!.isEmpty ? 'Required' : null,
                  ),
                  TextFormField(
                    controller: _initialBalanceController,
                    decoration: const InputDecoration(labelText: 'Initial/Current Balance'),
                    keyboardType: TextInputType.number,
                    validator: (value) => value!.isEmpty || double.tryParse(value) == null ? 'Enter a valid number' : null,
                  ),
                ],
              ),
            ),
          ),
          actions: [
            TextButton(onPressed: () => Navigator.of(context).pop(), child: const Text('Cancel')),
            ElevatedButton(onPressed: _createBankAccount, child: const Text('Link Account')),
          ],
        );
      },
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
                onPressed: _refreshBankAccounts,
              ),
            ],
          ),
          body: FutureBuilder<List<BankAccount>>(
            future: _bankAccountsFuture,
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
                    final account = snapshot.data![index];
                    return Card(
                      margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                      child: ListTile(
                        title: Text('${account.bankName} - ${account.accountName}'),
                        subtitle: Text('${account.accountType.toUpperCase()} | Balance: ${account.currency} ${account.currentBalance.toStringAsFixed(2)}'),
                        trailing: Icon(account.isSynced ? Icons.sync_enabled : Icons.sync_disabled),
                        onTap: () {
                          Navigator.of(context).push(MaterialPageRoute(
                            builder: (context) => BankAccountDetailPage(bankAccount: account),
                          ));
                        },
                      ),
                    );
                  },
                );
              }
            },
          ),
          floatingActionButton: FloatingActionButton(
            onPressed: _showCreateAccountDialog,
            child: const Icon(Icons.add),
          ),
        );
      }
    }
