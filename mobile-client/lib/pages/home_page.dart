import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/auth_service.dart';
import 'package:vimbai_mobile_client/pages/login_page.dart';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:vimbai_mobile_client/services/accounting_api_service.dart'; // NEW
import 'package:vimbai_mobile_client/pages/journal_entry_form_page.dart';
import 'package:vimbai_mobile_client/pages/budgets_page.dart';
import 'package:vimbai_mobile_client/pages/trial_balance_page.dart';
import 'package:vimbai_mobile_client/pages/balance_sheet_page.dart';
import 'package:vimbai_mobile_client/pages/multimodal_input_page.dart';
import 'package:vimbai_mobile_client/pages/books_page.dart';
import 'package:vimbai_mobile_client/pages/npo_page.dart';
import 'package:vimbai_mobile_client/pages/personal_finance_page.dart';
import 'package:vimbai_mobile_client/pages/bank_accounts_page.dart';
import 'package:vimbai_mobile_client/pages/financial_ratios_page.dart';

class HomePage extends StatefulWidget {
  const HomePage({super.key});

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  final AuthService _authService = AuthService();
  final AccountingApiService _accountingApiService = AccountingApiService(); // NEW
  ConnectivityResult _connectivityResult = ConnectivityResult.none;

  @override
  void initState() {
    super.initState();
    _checkConnectivity();
    Connectivity().onConnectivityChanged.listen((List<ConnectivityResult> results) {
      setState(() {
        _connectivityResult = results.isEmpty ? ConnectivityResult.none : results.last;
      });
      if (!results.contains(ConnectivityResult.none)) { // NEW: Attempt sync when online
        _syncOfflineData();
      }
    });
  }

  Future<void> _checkConnectivity() async {
    final results = await Connectivity().checkConnectivity();
    _connectivityResult = results.isEmpty ? ConnectivityResult.none : results.last;
    setState(() {});
  }

  Future<void> _syncOfflineData() async { // NEW
    if (_connectivityResult != ConnectivityResult.none) {
      try {
        await _accountingApiService.syncOfflineJournalEntries();
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('Offline Journal Entries synced successfully!')),
          );
        }
      } catch (e) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Error syncing offline data: ${e.toString()}')),
          );
        }
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Vimbai Home (Offline Ready)'),
            actions: [
              IconButton(
                icon: const Icon(Icons.logout),
                onPressed: () async {
                  await _authService.logout();
                  if (mounted) {
                    Navigator.of(context).pushReplacement(
                      MaterialPageRoute(builder: (context) => const LoginPage()),
                    );
                  }
                },
              ),
              IconButton( // NEW: Manual Sync Button
                icon: const Icon(Icons.cloud_upload),
                onPressed: _connectivityResult == ConnectivityResult.none ? null : _syncOfflineData,
                tooltip: 'Sync Offline Data',
              ),
            ],
          ),
          body: Center(
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  // ... (existing widgets) ...
                  const Text(
                    'Multimodal Input:',
                    style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 10),
                  ElevatedButton(
                    onPressed: () {
                      Navigator.of(context).push(
                        MaterialPageRoute(builder: (context) => const MultimodalInputPage()),
                      );
                    },
                    child: const Text('Process Image/Audio'),
                  ),
                  const SizedBox(height: 10),
                  ElevatedButton(
                    onPressed: () {
                      Navigator.of(context).push(
                        MaterialPageRoute(builder: (context) => const BooksPage()),
                      );
                    },
                    child: const Text('Your Books'),
                  ),
                  const SizedBox(height: 10),
                  ElevatedButton(
                    onPressed: () {
                      Navigator.of(context).push(
                        MaterialPageRoute(builder: (context) => const NpoPage()),
                      );
                    },
                    child: const Text('Non-profit Organizations'),
                  ),
                  const SizedBox(height: 10),
                  ElevatedButton(
                    onPressed: () {
                      Navigator.of(context).push(
                        MaterialPageRoute(
                            builder: (context) => const PersonalFinancePage()),
                      );
                    },
                    child: const Text('Personal finance'),
                  ),
                  const SizedBox(height: 20),
                  ElevatedButton(
                    onPressed: () {
                      Navigator.of(context).push(
                        MaterialPageRoute(builder: (context) => const FinancialRatiosPage()),
                      );
                    },
                    child: const Text('View Financial Ratios'),
                  ),
                  const SizedBox(height: 30),
                  const Text(
                    'Banking Integration:',
                    style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 10),
                  ElevatedButton(
                    onPressed: () {
                      Navigator.of(context).push(
                        MaterialPageRoute(builder: (context) => const BankAccountsPage()),
                      );
                    },
                    child: const Text('Manage Bank Accounts'),
                  ),
                ],
              ),
            ),
          ),
        );
      }
    }
