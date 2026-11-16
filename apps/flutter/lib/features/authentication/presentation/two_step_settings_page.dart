// Turning the second factor on and off from the phone.
//
// This is the half that makes the second factor real. The engine has always been able to raise a code, but it had nowhere to
// send one - the code only existed in an audit record - so an account that asked for a code was an account nobody could open.
// An authenticator app is the one channel that needs no mail server and no phone number, so this screen shows the key the
// engine hands over, and the person's own app makes the codes. The screen that *takes* a code at sign-in is a different one
// (`second_factor_page.dart`), because asking for a code and managing the app that makes them are different moments.
//
// The key is shown once, as text as well as an address: a phone displaying a QR code cannot scan it, and the code is what
// proves the app holds the key. Turning the factor off asks for a code too, so an unlocked phone is not enough.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';

class TwoStepSettingsPage extends StatefulWidget {
  const TwoStepSettingsPage({super.key, required this.authService});

  final AuthService authService;

  @override
  State<TwoStepSettingsPage> createState() => _TwoStepSettingsPageState();
}

class _TwoStepSettingsPageState extends State<TwoStepSettingsPage> {
  final TextEditingController _codeController = TextEditingController();

  bool _busy = false;
  bool _loadingState = true;
  bool? _enabled;
  SecondFactorEnrolment? _enrolment;
  String? _error;
  String? _notice;

  @override
  void initState() {
    super.initState();
    _readState();
  }

  @override
  void dispose() {
    _codeController.dispose();
    super.dispose();
  }

  Future<void> _readState() async {
    final enabled = await widget.authService.secondFactorEnabled();
    if (!mounted) return;
    setState(() {
      _loadingState = false;
      _enabled = enabled;
    });
  }

  Future<void> _beginEnrolment() async {
    setState(() {
      _busy = true;
      _error = null;
      _notice = null;
    });
    final enrolment = await widget.authService.beginSecondFactor();
    if (!mounted) return;
    setState(() {
      _busy = false;
      _enrolment = enrolment;
      _error = enrolment == null
          ? (widget.authService.lastResetError ??
                'The accounting engine could not start an enrolment.')
          : null;
    });
  }

  Future<void> _confirmCode() async {
    final code = _codeController.text.trim();
    if (code.isEmpty) {
      setState(() => _error = 'Enter the six digits your authenticator shows.');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
      _notice = null;
    });
    final turnedOn = await widget.authService.confirmSecondFactor(code);
    if (!mounted) return;
    setState(() {
      _busy = false;
      if (turnedOn) {
        _enabled = true;
        _enrolment = null;
        _codeController.clear();
        _notice =
            'Two-step sign-in is on. Next time you sign in, your authenticator app makes the six digits.';
      } else {
        _error =
            widget.authService.lastResetError ??
            'That code was not accepted. Check the app and try the current one.';
      }
    });
  }

  Future<void> _turnOff() async {
    final code = _codeController.text.trim();
    if (code.isEmpty) {
      setState(() => _error = 'Enter a code from your app to turn it off.');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
      _notice = null;
    });
    final turnedOff = await widget.authService.disableSecondFactor(code);
    if (!mounted) return;
    setState(() {
      _busy = false;
      if (turnedOff) {
        _enabled = false;
        _codeController.clear();
        _notice =
            'Two-step sign-in is off. Your password alone opens this account.';
      } else {
        _error =
            widget.authService.lastResetError ??
            'That code was not accepted, so the second factor is still on.';
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Two-step sign-in')),
      body: _loadingState
          ? const Center(child: CircularProgressIndicator())
          : SingleChildScrollView(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    _enabled == true
                        ? 'Two-step sign-in is on for this account.'
                        : 'Two-step sign-in is off: your password alone opens this account.',
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                  const SizedBox(height: 12),
                  if (_enabled == false && _enrolment == null) ...[
                    const Text(
                      'An authenticator app (Google Authenticator, Authy, 1Password, Bitwarden) makes six digits '
                      'every thirty seconds. It needs no email and no phone number, so it works on any phone.',
                    ),
                    const SizedBox(height: 16),
                    _busy
                        ? const CircularProgressIndicator()
                        : ElevatedButton(
                            onPressed: _beginEnrolment,
                            child: const Text('Turn on two-step sign-in'),
                          ),
                  ],
                  if (_enrolment != null) ...[
                    const Text(
                      'Add this key to your authenticator app, then enter the six digits it shows.',
                    ),
                    const SizedBox(height: 12),
                    SelectableText(
                      _enrolment!.secret,
                      style: const TextStyle(
                        fontFamily: 'monospace',
                        fontSize: 18,
                        letterSpacing: 2,
                      ),
                    ),
                    const SizedBox(height: 8),
                    const Text(
                      'If your app asks for an address instead, this is it:',
                    ),
                    const SizedBox(height: 4),
                    SelectableText(
                      _enrolment!.provisioningUri,
                      style: const TextStyle(
                        fontFamily: 'monospace',
                        fontSize: 12,
                      ),
                    ),
                    const SizedBox(height: 16),
                    TextField(
                      controller: _codeController,
                      keyboardType: TextInputType.number,
                      decoration: const InputDecoration(
                        labelText: 'Six digits from your app',
                      ),
                    ),
                    const SizedBox(height: 16),
                    _busy
                        ? const CircularProgressIndicator()
                        : ElevatedButton(
                            onPressed: _confirmCode,
                            child: const Text('Turn it on'),
                          ),
                  ],
                  if (_enabled == true) ...[
                    const Text(
                      'To turn it off, enter a code from your authenticator app. A code is asked for so that a phone '
                      'left unlocked on a table is not enough to take the second factor away.',
                    ),
                    const SizedBox(height: 16),
                    TextField(
                      controller: _codeController,
                      keyboardType: TextInputType.number,
                      decoration: const InputDecoration(
                        labelText: 'Six digits from your app',
                      ),
                    ),
                    const SizedBox(height: 16),
                    _busy
                        ? const CircularProgressIndicator()
                        : OutlinedButton(
                            onPressed: _turnOff,
                            child: const Text('Turn off two-step sign-in'),
                          ),
                  ],
                  if (_notice != null) ...[
                    const SizedBox(height: 16),
                    Text(_notice!),
                  ],
                  if (_error != null) ...[
                    const SizedBox(height: 16),
                    Text(
                      _error!,
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.error,
                      ),
                    ),
                  ],
                ],
              ),
            ),
    );
  }
}
