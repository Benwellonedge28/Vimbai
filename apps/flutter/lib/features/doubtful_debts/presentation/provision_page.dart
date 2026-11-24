// What the business expects not to be paid.
//
// Writing a debt off is a decision about one customer, made when the business stops expecting anything from them. A
// provision is the same worry said in advance and about all of them at once: some of what is owed will not come, and the
// business does not yet know which. A year that reports the whole of what is owed as coming reports a profit it may not
// have made.
//
// The screen asks what the provision should stand at, because the estimate is the person's: how much of what is owed will
// not come is a judgement about customers, and a phone that guessed a percentage would be inventing the judgement. It says
// what raising and lowering do to the books before anything is posted, says plainly that no customer's debt leaves the
// books and that nothing moves money, and it never works out the difference itself: the engine posts the movement and the
// screen repeats what it did.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/doubtful_debts/data/doubtful_debts_posting.dart';

class ProvisionPage extends StatefulWidget {
  const ProvisionPage({super.key, this.posting});

  /// A test passes its own service so the page can be exercised without a server.
  final DoubtfulDebtsPostingService? posting;

  @override
  State<ProvisionPage> createState() => _ProvisionPageState();
}

class _ProvisionPageState extends State<ProvisionPage> {
  late final DoubtfulDebtsPostingService _posting =
      widget.posting ?? DoubtfulDebtsPostingService.instance;

  final TextEditingController _day = TextEditingController(text: _today());
  final TextEditingController _figure = TextEditingController();
  final TextEditingController _reason = TextEditingController();

  ProvisionLadder _ladder = const ProvisionLadder(
    standing: '',
    adjustments: [],
  );
  bool _reading = true;
  bool _saving = false;
  String? _trouble;

  @override
  void initState() {
    super.initState();
    _read();
  }

  @override
  void dispose() {
    _day.dispose();
    _figure.dispose();
    _reason.dispose();
    super.dispose();
  }

  static String _today() {
    final now = DateTime.now();
    return '${now.year}-${now.month.toString().padLeft(2, '0')}-'
        '${now.day.toString().padLeft(2, '0')}';
  }

  Future<void> _read() async {
    setState(() {
      _reading = true;
      _trouble = null;
    });
    try {
      final ladder = await _posting.read();
      if (!mounted) return;
      setState(() {
        _ladder = ladder;
        _reading = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _reading = false;
        _trouble = CouldNotRead.reasonOf(failure);
      });
    }
  }

  /// Everything that would stop the engine posting, said before it is asked.
  String? get _refusal {
    if (!RegExp(r'^\d{4}-\d{2}-\d{2}$').hasMatch(_day.text.trim())) {
      return 'Say the day the provision was decided, as 2026-11-20.';
    }
    final typed = _figure.text.trim();
    if (typed.isEmpty) {
      return 'Say what the provision should stand at in money, as 450.00.';
    }
    final figure = double.tryParse(typed);
    if (figure == null) {
      return 'That is not a figure the books can carry. Say it as 450.00.';
    }
    if (figure <= 0) {
      return 'A provision has to be more than nothing. If nothing is doubtful, say so by lowering it to the figure it now stands at.';
    }
    if (_ladder.standing.trim().isNotEmpty) {
      final standing = double.tryParse(_ladder.standing.trim());
      if (standing != null && (standing - figure).abs() < 0.000001) {
        return 'The provision already stands at ${_ladder.standing}. An adjustment has to change the estimate.';
      }
    }
    return null;
  }

  Future<void> _provide() async {
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
      final recorded = await _posting.provide(
        entryDate: _day.text.trim(),
        provision: _figure.text.trim(),
        reason: _reason.text,
      );
      if (!mounted) return;
      setState(() {
        _ladder = ProvisionLadder(
          standing: recorded.provision,
          adjustments: [recorded, ..._ladder.adjustments],
        );
        _saving = false;
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
      appBar: AppBar(title: const Text('What you expect not to be paid')),
      body: _reading
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
              children: [
                Text(
                  'Some of what customers owe will not come, and you do not yet know which of them. This is the estimate '
                  'that stands against what is owed, so the year does not report a profit it may not have made.',
                  style: theme.textTheme.bodyMedium,
                ),
                const SizedBox(height: 12),
                Text(
                  _ladder.standing.trim().isEmpty
                      ? 'Nothing stands against receivables yet.'
                      : 'The provision stands at ${_ladder.standing} today. Only the difference is posted, never the '
                            'whole figure again.',
                  style: theme.textTheme.bodySmall,
                ),
                const SizedBox(height: 20),
                TextField(
                  controller: _day,
                  decoration: const InputDecoration(
                    labelText: 'The day it was decided',
                    helperText: 'As 2026-11-20: the month follows from it.',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) => setState(() {}),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _figure,
                  keyboardType: const TextInputType.numberWithOptions(
                    decimal: true,
                  ),
                  decoration: const InputDecoration(
                    labelText: 'What the provision should stand at',
                    helperText:
                        'In money, as 450.00: what you expect not to be paid, not what is owed.',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) => setState(() {}),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _reason,
                  decoration: const InputDecoration(
                    labelText: 'Why',
                    helperText:
                        'Optional: two customers have gone quiet, a big one is in dispute.',
                    border: OutlineInputBorder(),
                  ),
                ),
                const SizedBox(height: 20),
                FilledButton(
                  onPressed: _saving ? null : _provide,
                  child: Text(_saving ? 'Recording...' : 'Record the estimate'),
                ),
                const SizedBox(height: 8),
                Text(
                  'Raising it charges the month and grows what stands against receivables; lowering it takes the earlier '
                  'charge back. No customer\'s debt leaves the books, nobody is chased, and nothing moves money.',
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
                Text('Estimates made', style: theme.textTheme.titleSmall),
                const SizedBox(height: 6),
                if (_ladder.isEmpty)
                  Text(
                    'None yet: this Book has not provided for anything.',
                    style: theme.textTheme.bodySmall,
                  ),
                for (final one in _ladder.adjustments)
                  ListTile(
                    contentPadding: EdgeInsets.zero,
                    dense: true,
                    title: Text(
                      '${one.raised ? 'raised' : 'lowered'} by ${one.figure} to ${one.provision}',
                    ),
                    subtitle: Text(
                      '${one.entryDate} · was ${one.previous}'
                      '${one.reason.trim().isEmpty ? '' : ' · ${one.reason}'}',
                    ),
                  ),
              ],
            ),
    );
  }
}
