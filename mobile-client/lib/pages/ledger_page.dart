// mobile-client/lib/pages/ledger_page.dart

import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/accounting_api_service.dart';
import 'package:vimbai_mobile_client/models/accounting_models.dart';

class LedgerPage extends StatefulWidget {
  const LedgerPage({super.key});

  @override
  State<LedgerPage> createState() => _LedgerPageState();
}

class _LedgerPageState extends State<LedgerPage> {
  final TextEditingController _accountNumberController = TextEditingController();
  final AccountingApiService _apiService = AccountingApiService();
  LedgerAccountBalance? _ledgerBalance;
  bool _isLoading = false;
  String? _errorMessage;

  @override
  void dispose() {
    _accountNumberController.dispose();
    super.dispose();
  }

  Future<void> _fetchLedgerBalance() async {
    if (_accountNumberController.text.isEmpty) {
      setState(() {
        _errorMessage = 'Please enter an account number.';
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
      final balance = await _apiService.getLedgerAccountBalance(_accountNumberController.text.trim());
      setState(() {
        _ledgerBalance = balance;
      });
    } catch (e) {
      setState(() {
        _errorMessage = 'Error fetching ledger: ${e.toString()}';
      });
    } finally {
      setState(() {
        _isLoading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Ledger Account Balance'),
      ),
      body: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          children: [
            TextField(
              controller: _accountNumberController,
              decoration: InputDecoration(
                labelText: 'Account Number',
                suffixIcon: _isLoading
                    ? const Padding(
                        padding: EdgeInsets.all(8.0),
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : IconButton(
                        icon: const Icon(Icons.search),
                        onPressed: _fetchLedgerBalance,
                      ),
              ),
              keyboardType: TextInputType.number,
              onSubmitted: (_) => _fetchLedgerBalance(),
            ),
            const SizedBox(height: 20),
            if (_errorMessage != null)
              Text(_errorMessage!, style: const TextStyle(color: Colors.red)),
            if (_ledgerBalance != null)
              Card(
                margin: const EdgeInsets.symmetric(vertical: 10),
                child: Padding(
                  padding: const EdgeInsets.all(16.0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Account: ${_ledgerBalance!.accountNumber} - ${_ledgerBalance!.accountName}',
                        style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
                      ),
                      Text('Type: ${_ledgerBalance!.accountType}'),
                      Text('Normal Balance: ${_ledgerBalance!.normalBalance}'),
                      Text(
                        'Current Balance: \$${_ledgerBalance!.currentBalance.toStringAsFixed(2)}',
                        style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
                      ),
                    ],
                  ),
                ),
              ),
            const Spacer(),
            SizedBox(
              width: double.infinity,
              child: ElevatedButton(
                onPressed: _fetchLedgerBalance,
                child: const Text('Fetch Balance'),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
