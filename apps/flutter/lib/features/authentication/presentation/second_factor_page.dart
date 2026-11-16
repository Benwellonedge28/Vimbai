import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:vimbai_mobile_client/app/home_page.dart';
import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';

/// The second step of a sign-in the engine has already accepted the password for.
///
/// The code is entered here and checked by the engine; this screen decides nothing. A wrong or reused code is refused
/// there and the person is sent back to sign in again, because a challenge cannot be spent twice.
class SecondFactorPage extends StatefulWidget {
  const SecondFactorPage({super.key, required this.authService});

  final AuthService authService;

  @override
  State<SecondFactorPage> createState() => _SecondFactorPageState();
}

class _SecondFactorPageState extends State<SecondFactorPage> {
  final TextEditingController _codeController = TextEditingController();
  bool _isLoading = false;
  String? _errorMessage;

  Future<void> _verify() async {
    setState(() {
      _isLoading = true;
      _errorMessage = null;
    });

    final success = await widget.authService.completeSecondFactor(
      _codeController.text,
    );

    if (!mounted) return;
    setState(() {
      _isLoading = false;
      if (!success) {
        _errorMessage =
            widget.authService.lastError ?? 'That code was not accepted.';
      }
    });

    if (success) {
      Navigator.of(context).pushReplacement(
        MaterialPageRoute(builder: (context) => const HomePage()),
      );
    } else if (!widget.authService.awaitingSecondFactor) {
      // the challenge is spent, so the only way forward is a fresh sign-in
      Navigator.of(context).pop();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Second step')),
      body: Center(
        child: Padding(
          padding: const EdgeInsets.all(16.0),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Text(
                widget.authService.secondFactorMethod == null ||
                        widget.authService.secondFactorMethod == 'totp'
                    ? 'Enter the 6-digit code from your authenticator app.'
                    : 'Enter the code this account asked for.',
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 16),
              TextField(
                controller: _codeController,
                keyboardType: TextInputType.number,
                autofocus: true,
                // a code is sorted digits, so only digits are offered; the engine still has the final say
                inputFormatters: [
                  FilteringTextInputFormatter.digitsOnly,
                  LengthLimitingTextInputFormatter(10),
                ],
                decoration: InputDecoration(
                  labelText: 'Code',
                  errorText: _errorMessage,
                ),
                onSubmitted: (_) => _isLoading ? null : _verify(),
              ),
              const SizedBox(height: 24),
              _isLoading
                  ? const CircularProgressIndicator()
                  : ElevatedButton(
                      onPressed: _verify,
                      child: const Text('Verify'),
                    ),
            ],
          ),
        ),
      ),
    );
  }
}
