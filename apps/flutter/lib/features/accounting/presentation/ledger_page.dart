import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';

/// One account's history.
///
/// A stranger has no way of knowing an account number, so the account is picked from the Book's own chart rather than
/// typed from memory. Typing a number still works, because a person who knows their chart should not be slowed down.
class LedgerPage extends StatefulWidget {
  const LedgerPage({super.key, this.service});

  /// The engine this screen talks to. A test hands in its own so the screen can be read without a phone.
  final AccountingApiService? service;

  @override
  State<LedgerPage> createState() => _LedgerPageState();
}

class _LedgerPageState extends State<LedgerPage> {
  final TextEditingController _accountNumberController =
      TextEditingController();
  late final AccountingApiService _apiService =
      widget.service ?? AccountingApiService();
  LedgerAccountBalance? _ledgerBalance;
  List<Account> _accounts = const [];
  bool _isLoading = false;
  String? _errorMessage;
  String? _accountsError;

  @override
  void initState() {
    super.initState();
    _loadAccounts();
  }

  @override
  void dispose() {
    _accountNumberController.dispose();
    super.dispose();
  }

  Future<void> _loadAccounts() async {
    try {
      final accounts = await _apiService.getAllAccounts();
      if (mounted) {
        setState(() {
          _accounts = accounts;
          _accountsError = null;
        });
      }
    } catch (error) {
      // The chart is a convenience here, not the whole screen: a person who knows the number can still look it up, so a
      // failed read is said out loud and the field stays usable.
      if (mounted)
        setState(() => _accountsError = CouldNotRead.reasonOf(error));
    }
  }

  Future<void> _pickAccount() async {
    if (_accounts.isEmpty) await _loadAccounts();
    if (!mounted) return;
    final chosen = await showModalBottomSheet<Account>(
      context: context,
      isScrollControlled: true,
      builder: (context) => _AccountPicker(
        accounts: _accounts,
        error: _accountsError,
        onRetry: _loadAccounts,
      ),
    );
    if (chosen == null || !mounted) return;
    _accountNumberController.text = chosen.accountNumber;
    await _fetchLedgerBalance();
  }

  Future<void> _fetchLedgerBalance() async {
    final number = _accountNumberController.text.trim();
    if (number.isEmpty) {
      setState(() {
        _errorMessage =
            'Pick an account, or type its number, before asking for its history.';
        _ledgerBalance = null;
      });
      return;
    }

    setState(() {
      _isLoading = true;
      _errorMessage = null;
      _ledgerBalance = null;
    });

    try {
      final balance = await _apiService.getLedgerAccountBalance(number);
      if (mounted) setState(() => _ledgerBalance = balance);
    } catch (error) {
      if (mounted) {
        setState(() => _errorMessage = CouldNotRead.reasonOf(error));
      }
    } finally {
      if (mounted) setState(() => _isLoading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Ledger')),
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              controller: _accountNumberController,
              decoration: InputDecoration(
                labelText: 'Account number',
                helperText: _accounts.isEmpty
                    ? 'Type the account number, or pick one from the chart'
                    : 'Pick an account, or type its number',
                suffixIcon: _isLoading
                    ? const Padding(
                        padding: EdgeInsets.all(8.0),
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : IconButton(
                        icon: const Icon(Icons.search),
                        tooltip: 'Read this account',
                        onPressed: _fetchLedgerBalance,
                      ),
              ),
              onSubmitted: (_) => _fetchLedgerBalance(),
            ),
            const SizedBox(height: 8),
            OutlinedButton.icon(
              onPressed: _isLoading ? null : _pickAccount,
              icon: const Icon(Icons.account_tree_outlined),
              label: const Text('Pick from the chart of accounts'),
            ),
            const SizedBox(height: 20),
            if (_errorMessage != null)
              Text(
                _errorMessage!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            if (_ledgerBalance != null) _ledgerCard(_ledgerBalance!),
          ],
        ),
      ),
    );
  }

  Widget _ledgerCard(LedgerAccountBalance balance) {
    return Card(
      margin: const EdgeInsets.symmetric(vertical: 10),
      child: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '${balance.accountNumber} - ${balance.accountName}',
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
            ),
            Text('Type: ${balance.accountType}'),
            Text('Normal balance: ${balance.normalBalance}'),
            const SizedBox(height: 6),
            Text(
              'Balance: ${balance.currentBalance.toDecimalString()}',
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
            ),
          ],
        ),
      ),
    );
  }
}

/// The Book's chart, put in front of the person so an account can be recognized by its name and picked by its number.
class _AccountPicker extends StatelessWidget {
  const _AccountPicker({
    required this.accounts,
    required this.error,
    required this.onRetry,
  });

  final List<Account> accounts;
  final String? error;
  final Future<void> Function() onRetry;

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: SizedBox(
        height: MediaQuery.of(context).size.height * 0.7,
        child: Column(
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
              child: Row(
                children: [
                  const Expanded(
                    child: Text(
                      'Pick an account',
                      style: TextStyle(
                        fontSize: 18,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                  ),
                  TextButton(
                    onPressed: () => Navigator.of(context).pop(),
                    child: const Text('Close'),
                  ),
                ],
              ),
            ),
            Expanded(
              child: accounts.isEmpty
                  ? CouldNotRead(
                      what: 'The chart of accounts',
                      error: error,
                      onRetry: error == null ? null : () => onRetry(),
                    )
                  : ListView.builder(
                      itemCount: accounts.length,
                      itemBuilder: (context, index) {
                        final account = accounts[index];
                        return ListTile(
                          title: Text(
                            '${account.accountNumber} - ${account.accountName}',
                          ),
                          subtitle: Text(account.accountType),
                          onTap: () => Navigator.of(context).pop(account),
                        );
                      },
                    ),
            ),
          ],
        ),
      ),
    );
  }
}
