import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/services/auth_service.dart';
import 'package:finacc_mobile_client/pages/login_page.dart';
import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:finacc_mobile_client/pages/chart_of_accounts_page.dart'; // NEW
import 'package:finacc_mobile_client/pages/journal_entry_form_page.dart'; // NEW

class HomePage extends StatefulWidget {
  const HomePage({super.key});

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  final AuthService _authService = AuthService();
  ConnectivityResult _connectivityResult = ConnectivityResult.none;

  @override
  void initState() {
    super.initState();
    _checkConnectivity();
    Connectivity().onConnectivityChanged.listen((ConnectivityResult result) {
      setState(() {
        _connectivityResult = result;
      });
    });
  }

  Future<void> _checkConnectivity() async {
    _connectivityResult = await (Connectivity().checkConnectivity());
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('FinAcc Home (Offline Ready)'),
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
        ],
      ),
      body: Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            const Text(
              'Welcome to FinAcc!',
              style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold),
            ),
            const SizedBox(height: 20),
            Text(
              'Connectivity: ${_connectivityResult == ConnectivityResult.none ? 'Offline' : 'Online'}',
              style: TextStyle(
                fontSize: 18,
                color: _connectivityResult == ConnectivityResult.none ? Colors.red : Colors.green,
              ),
            ),
            const SizedBox(height: 30),
            ElevatedButton(
              onPressed: () {
                // Navigate to a page for offline data entry, e.g., Journal Entry
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('Simulating offline data entry...')),
                );
              },
              child: const Text('Go to Offline Data Entry'),
            ),
            const SizedBox(height: 20), // NEW
            ElevatedButton( // NEW
              onPressed: () {
                Navigator.of(context).push(
                  MaterialPageRoute(builder: (context) => const ChartOfAccountsPage()),
                );
              },
              child: const Text('View Chart of Accounts'),
            ),
            const SizedBox(height: 20), // NEW
            ElevatedButton( // NEW
              onPressed: () {
                Navigator.of(context).push(
                  MaterialPageRoute(builder: (context) => const JournalEntryFormPage()),
                );
              },
              child: const Text('Create Journal Entry'),
            ),
            // More FinAcc features will go here
          ],
        ),
      ),
    );
  }
}
