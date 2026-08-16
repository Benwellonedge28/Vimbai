// mobile-client/main.dart

import 'package:flutter/material.dart';
import 'package:provider/provider.dart'; // For dependency injection
import 'package:vimbai_mobile_client/local_db/local_database.dart';
import 'package:vimbai_mobile_client/services/sync_service.dart';
import 'package:vimbai_mobile_client/services/auth_service.dart';
import 'package:vimbai_mobile_client/services/accounting_api_service.dart';
import 'package:vimbai_mobile_client/services/multimodal_api_service.dart';
import 'package:vimbai_mobile_client/widgets/multimodal_entry_window.dart';
// Import other API services as they are implemented

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Initialize Local Database
  final localDatabase = LocalDatabaseImpl(); // Using the mock implementation for now
  await localDatabase.initialize();

  // Initialize API Services
  final authService = AuthService(); // Assuming a concrete AuthService implementation
  final accountingApiService = AccountingApiService();
  final multimodalApiService = MultimodalApiService();
  // Initialize other API services

  // Initialize Sync Service
  final syncService = SyncService(
    localDb: localDatabase,
    authService: authService,
    accountingApiService: accountingApiService,
    multimodalApiService: multimodalApiService,
    // Pass other initialized API services
  );

  // Start periodic sync when the app launches
  syncService.startPeriodicSync();

  runApp(
    MultiProvider(
      providers: [
        Provider<LocalDatabase>.value(value: localDatabase),
        Provider<AuthService>.value(value: authService),
        Provider<AccountingApiService>.value(value: accountingApiService),
        Provider<MultimodalApiService>.value(value: multimodalApiService),
        Provider<SyncService>.value(value: syncService),
        // Add providers for other services
      ],
      child: const VimbaiApp(),
    ),
  );
}

class VimbaiApp extends StatelessWidget {
  const VimbaiApp({Key? key}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Vimbai',
      theme: ThemeData(
        primarySwatch: Colors.blue,
      ),
      home: const MyHomePage(), // Or your actual home screen
    );
  }
}

class MyHomePage extends StatelessWidget {
  const MyHomePage({Key? key}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Vimbai Home'),
      ),
      body: Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: <Widget>[
            const Text('Welcome to Vimbai Mobile!'),
            ElevatedButton(
              onPressed: () {
                // Example of triggering manual sync
                Provider.of<SyncService>(context, listen: false).triggerManualSync();
              },
              child: const Text('Trigger Manual Sync'),
            ),
            ElevatedButton(
              onPressed: () {
                // Example of navigating to the MultimodalEntryWindow
                Navigator.push(
                  context,
                  MaterialPageRoute(builder: (context) => MultimodalEntryWindow()),
                );
              },
              child: const Text('Open Multimodal Entry'),
            ),
          ],
        ),
      ),
    );
  }
}
