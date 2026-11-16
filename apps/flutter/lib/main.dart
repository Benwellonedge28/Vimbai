import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/login_page.dart';
import 'package:vimbai_mobile_client/app/home_page.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';

void main() {
  // every request tops the session up first: an access token lasts an hour, and a phone left open past that used to keep
  // sending a dead one and calling itself signed in
  final auth = AuthService();
  ApiClient.beforeRequest = auth.refreshSession;
  runApp(const MyApp());
}

class MyApp extends StatefulWidget {
  const MyApp({super.key});

  @override
  State<MyApp> createState() => _MyAppState();
}

class _MyAppState extends State<MyApp> {
  final AuthService _authService = AuthService();
  bool _isLoggedIn = false;
  bool _isLoading = true;

  @override
  void initState() {
    super.initState();
    _checkLoginStatus();
  }

  Future<void> _checkLoginStatus() async {
    _isLoggedIn = await _authService.isLoggedIn();
    setState(() {
      _isLoading = false;
    });
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Vimbai Mobile Client',
      theme: ThemeData(primarySwatch: Colors.blue),
      home: _isLoading
          ? const Scaffold(body: Center(child: CircularProgressIndicator()))
          : (_isLoggedIn ? const HomePage() : const LoginPage()),
    );
  }
}
