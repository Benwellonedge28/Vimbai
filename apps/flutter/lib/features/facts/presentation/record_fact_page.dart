// Recording what happened, in the person's own words.
//
// Every journal screen before this one asked which two accounts to use. That is the bookkeeper's job, and it asks a person
// to know a chart they never wrote. Here they say what happened - money went out on fuel, money came in for a sale - and
// the engine finds the accounts in the Book's own chart and posts it.
//
// Two questions, and one of them is answered by a button rather than by the person: "Show me the entry" rehearses the
// posting so the accounts can be seen before anything is written, and "Record it" makes the same posting for real.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/facts/data/fact_posting.dart';

class RecordFactPage extends StatefulWidget {
  const RecordFactPage({super.key, this.posting});

  /// A test passes its own service so the page can be exercised without a server.
  final FactPostingService? posting;

  @override
  State<RecordFactPage> createState() => _RecordFactPageState();
}

class _RecordFactPageState extends State<RecordFactPage> {
  late final FactPostingService _posting =
      widget.posting ?? FactPostingService.instance;

  final TextEditingController _amount = TextEditingController();
  final TextEditingController _whatFor = TextEditingController();
  final TextEditingController _money = TextEditingController();
  final TextEditingController _note = TextEditingController();

  FactKind _kind = FactKind.spent;
  DateTime _happenedOn = DateTime.now();

  bool _working = false;
  String? _complaint;
  FactAnswer? _answer;

  @override
  void dispose() {
    _amount.dispose();
    _whatFor.dispose();
    _money.dispose();
    _note.dispose();
    super.dispose();
  }

  String get _day =>
      '${_happenedOn.year.toString().padLeft(4, '0')}-'
      '${_happenedOn.month.toString().padLeft(2, '0')}-'
      '${_happenedOn.day.toString().padLeft(2, '0')}';

  /// What the person is being asked for, in the words of the fact they chose.
  String get _whatForLabel =>
      _kind == FactKind.spent ? 'What it was spent on' : 'What it came in for';

  String get _whatForHint => _kind == FactKind.spent
      ? 'Rent, fuel, airtime: whatever the Book calls it'
      : 'Sales, or whatever the Book calls this trade';

  /// Ask the engine, rehearsing unless [rehearse] is false.
  Future<void> _ask({required bool rehearse}) async {
    final amount = ExactAmount.tryParse(_amount.text);
    if (amount == null || amount.isZero) {
      setState(() {
        _complaint = 'Say how much, like 40.50.';
        _answer = null;
      });
      return;
    }
    if (_whatFor.text.trim().isEmpty) {
      setState(() {
        _complaint =
            'Say $_whatForLabel with a word or two, so the engine can find the account.';
        _answer = null;
      });
      return;
    }
    setState(() {
      _working = true;
      _complaint = null;
    });
    try {
      final answer = rehearse
          ? await _posting.rehearse(
              kind: _kind,
              entryDate: _day,
              amount: amount,
              account: _whatFor.text,
              money: _money.text,
              note: _note.text,
            )
          : await _posting.post(
              kind: _kind,
              entryDate: _day,
              amount: amount,
              account: _whatFor.text,
              money: _money.text,
              note: _note.text,
            );
      if (!mounted) return;
      setState(() {
        _answer = answer;
        _working = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _complaint = error.toString().replaceFirst('Exception: ', '');
        _answer = null;
        _working = false;
      });
    }
  }

  Future<void> _pickDay() async {
    final chosen = await showDatePicker(
      context: context,
      initialDate: _happenedOn,
      firstDate: DateTime(2000),
      lastDate: DateTime.now().add(const Duration(days: 365)),
    );
    if (chosen != null && mounted) setState(() => _happenedOn = chosen);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Record what happened')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            'Say what happened. The accounts come from your own chart.',
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 16),
          SegmentedButton<FactKind>(
            segments: const [
              ButtonSegment(
                value: FactKind.spent,
                label: Text('Money went out'),
                icon: Icon(Icons.south_east),
              ),
              ButtonSegment(
                value: FactKind.received,
                label: Text('Money came in'),
                icon: Icon(Icons.north_east),
              ),
            ],
            selected: {_kind},
            onSelectionChanged: (chosen) => setState(() {
              _kind = chosen.first;
              _answer = null;
            }),
          ),
          const SizedBox(height: 16),
          ListTile(
            contentPadding: EdgeInsets.zero,
            leading: const Icon(Icons.event_outlined),
            title: const Text('The day it happened'),
            subtitle: Text(_day),
            trailing: const Icon(Icons.edit_calendar_outlined),
            onTap: _pickDay,
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _amount,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'How much',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _whatFor,
            decoration: InputDecoration(
              labelText: _whatForLabel,
              hintText: _whatForHint,
              border: const OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _money,
            decoration: const InputDecoration(
              labelText: 'Which account the money moved through',
              hintText: 'Leave empty if the Book keeps only one',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _note,
            decoration: const InputDecoration(
              labelText: 'A note, if it helps later',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 16),
          Row(
            children: [
              Expanded(
                child: OutlinedButton(
                  onPressed: _working ? null : () => _ask(rehearse: true),
                  child: const Text('Show me the entry'),
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: FilledButton(
                  onPressed: _working ? null : () => _ask(rehearse: false),
                  child: Text(_working ? 'Working' : 'Record it'),
                ),
              ),
            ],
          ),
          if (_complaint != null) ...[
            const SizedBox(height: 16),
            Card(
              color: theme.colorScheme.errorContainer,
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Text(
                  _complaint!,
                  style: TextStyle(color: theme.colorScheme.onErrorContainer),
                ),
              ),
            ),
          ],
          if (_answer != null) ...[
            const SizedBox(height: 16),
            _Posting(answer: _answer!),
          ],
        ],
      ),
    );
  }
}

/// The posting the engine derived, shown as a person reads it.
class _Posting extends StatelessWidget {
  const _Posting({required this.answer});

  final FactAnswer answer;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              answer.posted ? 'Recorded' : 'This is what would be posted',
              style: theme.textTheme.titleMedium,
            ),
            const SizedBox(height: 8),
            Text(answer.description, style: theme.textTheme.bodyMedium),
            const SizedBox(height: 12),
            _line(theme, 'Debit', answer.debit.label),
            _line(theme, 'Credit', answer.credit.label),
            _line(theme, 'Amount', answer.amount),
            _line(theme, 'Day', answer.entryDate),
            if (answer.posted && answer.entryId.isNotEmpty)
              _line(theme, 'Entry', answer.entryId),
            const SizedBox(height: 8),
            Text(
              answer.posted
                  ? 'The books hold it now.'
                  : 'Nothing has been recorded yet.',
              style: theme.textTheme.bodySmall,
            ),
          ],
        ),
      ),
    );
  }

  Widget _line(ThemeData theme, String label, String value) => Padding(
    padding: const EdgeInsets.only(bottom: 4),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 84,
          child: Text(
            label,
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ),
        Expanded(child: Text(value, style: theme.textTheme.bodyMedium)),
      ],
    ),
  );
}
