import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';

/// Change the password this account signs in with.
///
/// The current password is asked for because the engine demands it: a phone left unlocked on a table must not be enough to
/// take the account over. The new one is checked by the engine against the same rule it applies at registration and at
/// reset, and its refusal is shown as it comes.
///
/// A successful change ends this session - the engine retires every refresh token the person holds - so this screen says
/// that plainly and hands back to sign-in rather than leaving the person to discover it later.
class ChangePasswordPage extends StatefulWidget {
  const ChangePasswordPage({super.key, required this.authService});

  final AuthService authService;

  @override
  State<ChangePasswordPage> createState() => _ChangePasswordPageState();
}

class _ChangePasswordPageState extends State<ChangePasswordPage> {
  final TextEditingController _currentController = TextEditingController();
  final TextEditingController _newController = TextEditingController();
  final TextEditingController _againController = TextEditingController();

  bool _isLoading = false;
  String? _error;

  Future<void> _submit() async {
    if (_currentController.text.isEmpty) {
      setState(() => _error = 'Enter your current password.');
      return;
    }
    if (_newController.text != _againController.text) {
      setState(() => _error = 'The two new passwords do not match.');
      return;
    }
    setState(() {
      _isLoading = true;
      _error = null;
    });
    final changed = await widget.authService.changePassword(
      currentPassword: _currentController.text,
      newPassword: _newController.text,
    );
    if (!mounted) return;
    if (!changed) {
      setState(() {
        _isLoading = false;
        _error =
            widget.authService.lastResetError ??
            'That password could not be changed. Try again.';
      });
      return;
    }
    setState(() => _isLoading = false);
    Navigator.of(context).pop(true);
  }

  @override
  void dispose() {
    _currentController.dispose();
    _newController.dispose();
    _againController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Change password')),
      body: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(16.0),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              TextField(
                controller: _currentController,
                obscureText: true,
                decoration: const InputDecoration(
                  labelText: 'Current password',
                ),
              ),
              const SizedBox(height: 12),
              TextField(
                controller: _newController,
                obscureText: true,
                decoration: const InputDecoration(
                  labelText: 'New password',
                  helperText: 'At least 8 characters',
                ),
              ),
              const SizedBox(height: 12),
              TextField(
                controller: _againController,
                obscureText: true,
                decoration: const InputDecoration(
                  labelText: 'New password again',
                ),
              ),
              const SizedBox(height: 20),
              _isLoading
                  ? const CircularProgressIndicator()
                  : ElevatedButton(
                      onPressed: _submit,
                      child: const Text('Change the password'),
                    ),
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
