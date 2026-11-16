import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/app/home_page.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/register_page.dart';

class LoginPage extends StatefulWidget {
  const LoginPage({super.key});

  @override
  State<LoginPage> createState() => _LoginPageState();
}

class _LoginPageState extends State<LoginPage> {
  final TextEditingController _usernameController = TextEditingController();
  final TextEditingController _passwordController = TextEditingController();
  final AuthService _authService = AuthService();
  bool _isLoading = false;
  String? _errorMessage;

  Future<void> _login() async {
    setState(() {
      _isLoading = true;
      _errorMessage = null;
    });

    final username = _usernameController.text;
    final password = _passwordController.text;

    // One path to the books: the engine checks the password and answers. There is deliberately no fallback that turns a
    // refused password into a session - the shipped page did exactly that, treating a cached profile on the phone as
    // enough to sign in when the engine said no, so a wrong password opened a device that had been used before. Cached
    // data is for reading what is already on the phone, not for deciding who someone is.
    final success = await _authService.login(username, password);
    if (!success) {
      // the engine's own reason, in the words the service chose for a person to read; a cause it cannot name falls back
      // to the sentence this page always used
      _errorMessage =
          _authService.lastError ??
          'Sign-in failed. Check your username and password, and that the accounting engine is running.';
    }

    setState(() {
      _isLoading = false;
    });

    if (success && mounted) {
      Navigator.of(context).pushReplacement(
        MaterialPageRoute(builder: (context) => const HomePage()),
      );
    } else if (_errorMessage != null && mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(_errorMessage!)));
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Vimbai Login')),
      body: Center(
        child: Padding(
          padding: const EdgeInsets.all(16.0),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              TextField(
                controller: _usernameController,
                decoration: const InputDecoration(labelText: 'Username'),
              ),
              const SizedBox(height: 12),
              TextField(
                controller: _passwordController,
                obscureText: true,
                decoration: const InputDecoration(labelText: 'Password'),
              ),
              const SizedBox(height: 24),
              _isLoading
                  ? const CircularProgressIndicator()
                  : ElevatedButton(
                      onPressed: _login,
                      child: const Text('Login'),
                    ),
              TextButton(
                onPressed: () {
                  Navigator.of(context).push(
                    MaterialPageRoute(
                      builder: (context) => const RegisterPage(),
                    ),
                  );
                },
                child: const Text('Don\'t have an account? Register'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
