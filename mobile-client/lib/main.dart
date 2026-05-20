import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/pages/login_page.dart';
import 'package:finacc_mobile_client/pages/home_page.dart';
import 'package:finacc_mobile_client/services/auth_service.dart';

void main() {
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
      title: 'FinAcc Mobile Client',
      theme: ThemeData(
        primarySwatch: Colors.blue,
      ),
      home: _isLoading
          ? const Scaffold(body: Center(child: CircularProgressIndicator()))
          : (_isLoggedIn ? const HomePage() : const LoginPage()),
    );
  }
}
