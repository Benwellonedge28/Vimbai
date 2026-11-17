import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/app/home_page.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/password_reset_page.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/register_page.dart';
import 'package:vimbai_mobile_client/features/authentication/presentation/second_factor_page.dart';

class LoginPage extends StatefulWidget {
  const LoginPage({super.key, this.initialUsername, this.authService});

  /// The name to start from. Registration hands it over, so the person types only the password they just chose rather
  /// than remembering the exact spelling of a name they made up a minute ago.
  final String? initialUsername;

  /// Who signs people in. A test hands this one an engine that answers without a phone.
  final AuthService? authService;

  @override
  State<LoginPage> createState() => _LoginPageState();
}

class _LoginPageState extends State<LoginPage> {
  late final TextEditingController _usernameController = TextEditingController(
    text: widget.initialUsername ?? '',
  );
  final TextEditingController _passwordController = TextEditingController();
  late final AuthService _authService = widget.authService ?? AuthService();
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
    } else if (_authService.awaitingSecondFactor && mounted) {
      // the password was right and the account owes its second step; that is a step forward, not an error
      Navigator.of(context).push(
        MaterialPageRoute(
          builder: (context) => SecondFactorPage(authService: _authService),
        ),
      );
    } else if (_errorMessage != null && mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(_errorMessage!)));
    }
  }

  /// A forgotten password is not a dead end any more: the engine sends a reset link, and this screen takes the code back.
  Future<void> _forgotPassword() async {
    final changed = await Navigator.of(context).push<bool>(
      MaterialPageRoute(
        builder: (context) => PasswordResetPage(authService: _authService),
      ),
    );
    if (changed == true && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('Password changed. Sign in with your new password.'),
        ),
      );
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
                onPressed: _forgotPassword,
                child: const Text('Forgot your password?'),
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
