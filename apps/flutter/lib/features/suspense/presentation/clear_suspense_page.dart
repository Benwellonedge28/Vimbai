// Money that could not be placed, moved to where it belongs.
//
// Suspense holds an amount the books could not put anywhere when it was recorded: money received from somebody not yet
// known, or a payment made before it was clear what it was for. It is a question, not an answer, and the answer is a
// person's: what the amount actually was, and which account it belongs in.
//
// The screen asks for the day, the account it belongs in, the amount and which side that account takes, and it says what
// each side does to the books before it is posted. It says plainly that nothing moves money: suspense is a bookkeeping
// place, not an account anybody holds.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/suspense/data/suspense_clearing.dart';

class ClearSuspensePage extends StatefulWidget {
  const ClearSuspensePage({super.key, this.clearing});

  /// A test passes its own service so the page can be exercised without a server.
  final SuspenseClearingService? clearing;

  @override
  State<ClearSuspensePage> createState() => _ClearSuspensePageState();
}

class _ClearSuspensePageState extends State<ClearSuspensePage> {
  late final SuspenseClearingService _clearing =
      widget.clearing ?? SuspenseClearingService.instance;

  final TextEditingController _day = TextEditingController(text: _today());
  final TextEditingController _account = TextEditingController();
  final TextEditingController _figure = TextEditingController();
  final TextEditingController _reason = TextEditingController();

  /// `credit` when the account it belongs in is credited, `debit` when it is debited.
  String _side = 'credit';

  /// What this screen has cleared since it was opened.
  final List<SuspenseClearing> _cleared = [];

  bool _saving = false;
  String? _trouble;

  @override
  void dispose() {
    _day.dispose();
    _account.dispose();
    _figure.dispose();
    _reason.dispose();
    super.dispose();
  }

  static String _today() {
    final now = DateTime.now();
    return '${now.year}-${now.month.toString().padLeft(2, '0')}-'
        '${now.day.toString().padLeft(2, '0')}';
  }

  bool get _credited => _side == 'credit';

  /// Everything that would stop the engine posting, said before it is asked.
  String? get _refusal {
    if (!RegExp(r'^\d{4}-\d{2}-\d{2}$').hasMatch(_day.text.trim())) {
      return 'Say the day the clearing belongs to, as 2026-11-20.';
    }
    if (_account.text.trim().isEmpty) {
      return 'Say which account the amount belongs in: its number, as 4000.';
    }
    final typed = _figure.text.trim();
    if (typed.isEmpty) {
      return 'Say what the amount comes to, as 500.00.';
    }
    final figure = double.tryParse(typed);
    if (figure == null) {
      return 'That is not a figure the books can carry. Say it as 500.00.';
    }
    if (figure <= 0) {
      return 'A clearing has to be more than nothing: say what the amount comes to.';
    }
    return null;
  }

  Future<void> _record() async {
    final refusal = _refusal;
    if (refusal != null) {
      setState(() => _trouble = refusal);
      return;
    }
    setState(() {
      _saving = true;
      _trouble = null;
    });
    try {
      final cleared = await _clearing.clear(
        entryDate: _day.text.trim(),
        accountNumber: _account.text.trim(),
        amount: _figure.text.trim(),
        side: _side,
        reason: _reason.text,
      );
      if (!mounted) return;
      setState(() {
        _cleared.insert(0, cleared);
        _saving = false;
        _account.clear();
        _figure.clear();
        _reason.clear();
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _trouble = CouldNotRead.reasonOf(failure);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Money that could not be placed')),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
        children: [
          Text(
            'Suspense holds an amount the books could not put anywhere when it was recorded. It is a question until '
            'somebody says what the amount actually was.',
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 20),
          SegmentedButton<String>(
            segments: const [
              ButtonSegment(
                value: 'credit',
                label: Text('It was earned or owed'),
              ),
              ButtonSegment(value: 'debit', label: Text('It was spent')),
            ],
            selected: {_side},
            onSelectionChanged: (chosen) => setState(() {
              _side = chosen.first;
              _trouble = null;
            }),
          ),
          const SizedBox(height: 8),
          Text(
            _credited
                ? 'The account it belongs in is credited, and suspense gives the amount up: money came in, and this is '
                      'what it was. Sales, or a customer who now owes it.'
                : 'The account it belongs in is debited, and suspense gives the amount up: money went out, and this is '
                      'what it paid for. An expense, or something the business now owns.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 20),
          TextField(
            controller: _day,
            decoration: const InputDecoration(
              labelText: 'The day it belongs to',
              helperText: 'As 2026-11-20: the month follows from it.',
              border: OutlineInputBorder(),
            ),
            onChanged: (value) => setState(() {}),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _account,
            decoration: const InputDecoration(
              labelText: 'The account it belongs in',
              helperText: 'Its number in this Book, as 4000 for Sales.',
              border: OutlineInputBorder(),
            ),
            onChanged: (value) => setState(() {}),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _figure,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'What the amount comes to',
              helperText: 'In money, as 500.00.',
              border: OutlineInputBorder(),
            ),
            onChanged: (value) => setState(() {}),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _reason,
            decoration: const InputDecoration(
              labelText: 'What it was',
              helperText:
                  'Optional: the customer was identified, the invoice arrived.',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 20),
          FilledButton(
            onPressed: _saving ? null : _record,
            child: Text(_saving ? 'Clearing...' : 'Clear it out of suspense'),
          ),
          const SizedBox(height: 8),
          Text(
            'Nothing moves: suspense is a bookkeeping place, not an account anybody holds.',
            style: theme.textTheme.bodySmall,
          ),
          if (_trouble != null) ...[
            const SizedBox(height: 16),
            Text(
              _trouble!,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.error,
              ),
            ),
          ],
          const SizedBox(height: 24),
          Text(
            'Cleared since this screen was opened',
            style: theme.textTheme.titleSmall,
          ),
          const SizedBox(height: 6),
          if (_cleared.isEmpty)
            Text('Nothing yet.', style: theme.textTheme.bodySmall),
          for (final one in _cleared)
            ListTile(
              contentPadding: EdgeInsets.zero,
              dense: true,
              title: Text('${one.accountName} (${one.accountNumber})'),
              subtitle: Text(
                '${one.entryDate} · ${one.amount} ${one.credited ? 'credited' : 'debited'}, '
                'suspense ${one.suspenseTook} · ${one.reference}',
              ),
              trailing: const Icon(Icons.check, size: 18),
            ),
        ],
      ),
    );
  }
}

extension on SuspenseClearing {
  bool get credited => side == 'credit';
}
