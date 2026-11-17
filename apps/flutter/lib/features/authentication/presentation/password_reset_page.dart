import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';

/// Getting back in after forgetting a password.
///
/// Two steps, because a reset is two different acts: asking the engine for a link, then presenting the code from it with a
/// new password. Neither step decides anything - the engine says whether the address is known (in the same words either
/// way, so this door cannot be used to discover who has an account), whether the code is still good, and whether the new
/// password is acceptable. This screen shows those words as they come.
class PasswordResetPage extends StatefulWidget {
  const PasswordResetPage({super.key, required this.authService});

  final AuthService authService;

  @override
  State<PasswordResetPage> createState() => _PasswordResetPageState();
}

class _PasswordResetPageState extends State<PasswordResetPage> {
  final TextEditingController _emailController = TextEditingController();
  final TextEditingController _linkController = TextEditingController();
  final TextEditingController _passwordController = TextEditingController();

  bool _isLoading = false;
  bool _linkRequested = false;
  String? _error;
  String? _said;

  Future<void> _requestLink() async {
    final email = _emailController.text.trim();
    if (email.isEmpty) {
      setState(() => _error = 'Enter the email address on the account.');
      return;
    }
    setState(() {
      _isLoading = true;
      _error = null;
      _said = null;
    });
    final sent = await widget.authService.requestPasswordReset(email);
    if (!mounted) return;
    setState(() {
      _isLoading = false;
      if (sent) {
        _linkRequested = true;
        // the engine's own conditional sentence, never "we sent it": whether that address has an account is not this
        // screen's information to reveal
        _said = widget.authService.lastResetMessage;
      } else {
        _error =
            widget.authService.lastResetError ??
            'That request could not be sent. Try again.';
      }
    });
  }

  Future<void> _confirm() async {
    final token = AuthService.resetTokenFrom(_linkController.text);
    if (token.isEmpty) {
      setState(() => _error = 'Paste the reset code you were given.');
      return;
    }
    if (_passwordController.text.isEmpty) {
      setState(() => _error = 'Choose a new password.');
      return;
    }
    setState(() {
      _isLoading = true;
      _error = null;
      _said = null;
    });
    final changed = await widget.authService.confirmPasswordReset(
      token: token,
      newPassword: _passwordController.text,
    );
    if (!mounted) return;
    if (!changed) {
      setState(() {
        _isLoading = false;
        _error =
            widget.authService.lastResetError ??
            'That reset code was not accepted. Ask for a new one.';
      });
      return;
    }
    setState(() => _isLoading = false);
    Navigator.of(context).pop(true);
  }

  @override
  void dispose() {
    _emailController.dispose();
    _linkController.dispose();
    _passwordController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Reset password')),
      body: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(16.0),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              TextField(
                controller: _emailController,
                keyboardType: TextInputType.emailAddress,
                decoration: const InputDecoration(labelText: 'Email address'),
              ),
              const SizedBox(height: 16),
              _isLoading
                  ? const CircularProgressIndicator()
                  : ElevatedButton(
                      onPressed: _requestLink,
                      child: const Text('Ask for a reset code'),
                    ),
              if (_said != null) ...[
                const SizedBox(height: 16),
                Text(_said!, textAlign: TextAlign.center),
              ],
              if (_linkRequested) ...[
                const SizedBox(height: 28),
                TextField(
                  controller: _linkController,
                  decoration: const InputDecoration(
                    labelText: 'Reset code or link',
                    helperText:
                        'Paste the code you were given, or the whole link',
                  ),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _passwordController,
                  obscureText: true,
                  decoration: const InputDecoration(
                    labelText: 'New password',
                    helperText: 'At least 8 characters',
                  ),
                ),
                const SizedBox(height: 16),
                ElevatedButton(
                  onPressed: _isLoading ? null : _confirm,
                  child: const Text('Set the new password'),
                ),
              ],
              if (_error != null) ...[
                const SizedBox(height: 16),
                Text(
                  _error!,
                  textAlign: TextAlign.center,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
