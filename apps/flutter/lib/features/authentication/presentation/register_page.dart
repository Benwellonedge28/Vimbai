import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/login_page.dart';

class RegisterPage extends StatefulWidget {
  const RegisterPage({super.key, this.authService});

  /// Who makes the account. A test hands this one an engine that answers without a phone.
  final AuthService? authService;

  @override
  State<RegisterPage> createState() => _RegisterPageState();
}

class _RegisterPageState extends State<RegisterPage> {
  final TextEditingController _usernameController = TextEditingController();
  final TextEditingController _emailController = TextEditingController();
  final TextEditingController _passwordController = TextEditingController();
  late final AuthService _authService = widget.authService ?? AuthService();
  bool _isLoading = false;
  String? _errorMessage;

  Future<void> _register() async {
    setState(() {
      _isLoading = true;
      _errorMessage = null;
    });

    final username = _usernameController.text;
    final email = _emailController.text;
    final password = _passwordController.text;

    // The engine decides what a password may be; this only saves a round trip for the one rule a person can see for
    // themselves. Deliberately no complexity rule, because the engine has none: length is the requirement, so telling
    // someone to add a symbol here would be inventing a rule the engine does not hold them to.
    if (password.runes.length < 8) {
      setState(() {
        _isLoading = false;
        _errorMessage = 'Use at least 8 characters.';
      });
      return;
    }

    final success = await _authService.register(
      username,
      email,
      password,
      'user',
    );

    setState(() {
      _isLoading = false;
    });

    if (success && mounted) {
      // An account is not a session: the engine answers with the account and no token, so signing in is the next step
      // and not something to skip. The name is carried over so the person types only the password they just chose.
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('Account created. Sign in to open your Books.'),
        ),
      );
      Navigator.of(context).pushReplacement(
        MaterialPageRoute(
          builder: (context) => LoginPage(initialUsername: username),
        ),
      );
    } else if (mounted) {
      _errorMessage =
          _authService.lastError ?? 'Registration failed. Please try again.';
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Vimbai Register')),
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
                controller: _emailController,
                keyboardType: TextInputType.emailAddress,
                decoration: const InputDecoration(labelText: 'Email'),
              ),
              const SizedBox(height: 12),
              TextField(
                controller: _passwordController,
                obscureText: true,
                decoration: const InputDecoration(
                  labelText: 'Password',
                  helperText:
                      'At least 8 characters. A few words you will '
                      'remember is stronger than a short word with symbols.',
                  helperMaxLines: 2,
                ),
              ),
              const SizedBox(height: 24),
              _isLoading
                  ? const CircularProgressIndicator()
                  : ElevatedButton(
                      onPressed: _register,
                      child: const Text('Register'),
                    ),
              // A refusal has to be visible where the person is looking. It used to be set and never shown for the one
              // check this page makes itself, so a short password answered a tap with nothing at all.
              if (_errorMessage != null)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: Text(
                    _errorMessage!,
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.error,
                    ),
                  ),
                ),
              TextButton(
                onPressed: () {
                  Navigator.of(context).pop();
                },
                child: const Text('Already have an account? Login'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
