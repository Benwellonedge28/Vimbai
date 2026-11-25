// A bank statement, matched against what the cash book holds.
//
// The screen asks for the three things only the person has: which account, the balance the statement ends on, and its
// lines. Everything else is the engine's - which line is the same movement as which entry, what each side holds alone,
// and the proof that the difference between the two balances is exactly those lines.
//
// The answer is read as a proof rather than a verdict: the two balances, the difference, and what is left of it once the
// lines each side holds alone are counted. A difference that is fully explained says so in the engine's words; one that
// is not shows what remains, because a reconciliation that quietly rounds a gap away is worse than no reconciliation.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/bank_reconciliation/data/bank_reconciliation_service.dart';

class BankReconciliationPage extends StatefulWidget {
  const BankReconciliationPage({super.key, this.service});

  /// A test passes its own service so the page can be exercised without a server.
  final BankReconciliationService? service;

  @override
  State<BankReconciliationPage> createState() => _BankReconciliationPageState();
}

class _BankReconciliationPageState extends State<BankReconciliationPage> {
  late final BankReconciliationService _service =
      widget.service ?? BankReconciliationService.instance;

  final _account = TextEditingController();
  final _balance = TextEditingController();
  final List<StatementLine> _lines = [];

  Reconciliation? _answer;
  Object? _failure;
  bool _matching = false;
  String? _trouble;

  @override
  void dispose() {
    _account.dispose();
    _balance.dispose();
    super.dispose();
  }

  Future<void> _match() async {
    final account = _account.text.trim();
    final balance = ExactAmount.tryParse(_balance.text.trim());
    if (account.isEmpty) {
      setState(
        () => _trouble = 'Say which bank account this statement is for.',
      );
      return;
    }
    if (balance == null) {
      setState(
        () => _trouble =
            'That is not a balance the books can carry. Say it as 1287.50.',
      );
      return;
    }
    if (_lines.isEmpty) {
      setState(
        () => _trouble =
            'A statement with no lines has nothing to match. Add its lines first.',
      );
      return;
    }
    setState(() {
      _matching = true;
      _trouble = null;
      _failure = null;
    });
    try {
      final answer = await _service.match(
        account: account,
        statementBalance: _balance.text.trim(),
        lines: List.unmodifiable(_lines),
      );
      if (!mounted) return;
      setState(() {
        _answer = answer;
        _matching = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _failure = error;
        _matching = false;
      });
    }
  }

  void _readAnother() {
    setState(() {
      _answer = null;
      _failure = null;
      _lines.clear();
    });
  }

  Future<void> _addLine() async {
    final line = await showModalBottomSheet<StatementLine>(
      context: context,
      isScrollControlled: true,
      builder: (sheet) => const _LineSheet(),
    );
    if (line == null) return;
    setState(() {
      _lines.add(line);
      _trouble = null;
    });
  }

  @override
  Widget build(BuildContext context) {
    final answer = _answer;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Bank statement'),
        actions: [
          if (answer != null)
            TextButton(onPressed: _readAnother, child: const Text('Another')),
        ],
      ),
      body: answer != null
          ? _result(context, answer)
          : (_failure != null ? _refused(context) : _form(context)),
    );
  }

  /// The engine's own words when it refuses the statement, as the whole answer rather than a note under the form.
  Widget _refused(BuildContext context) => CouldNotRead(
    what: 'the reconciliation',
    error: _failure,
    onRetry: _match,
  );

  Widget _form(BuildContext context) {
    final theme = Theme.of(context);
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
      children: [
        TextField(
          controller: _account,
          decoration: const InputDecoration(
            labelText: 'Bank account code',
            hintText: '1020',
            border: OutlineInputBorder(),
          ),
        ),
        const SizedBox(height: 12),
        TextField(
          controller: _balance,
          keyboardType: const TextInputType.numberWithOptions(decimal: true),
          decoration: const InputDecoration(
            labelText: 'Balance the statement ends on',
            hintText: '1287.50',
            border: OutlineInputBorder(),
          ),
        ),
        const SizedBox(height: 16),
        Row(
          children: [
            Expanded(
              child: Text(
                'The statement\'s lines',
                style: theme.textTheme.titleSmall,
              ),
            ),
            TextButton.icon(
              onPressed: _addLine,
              icon: const Icon(Icons.add, size: 18),
              label: const Text('Add a line'),
            ),
          ],
        ),
        if (_lines.isEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 8),
            child: Text(
              'No lines yet. Type them as the bank sent them, including the ones you think are not in your books.',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
        for (var index = 0; index < _lines.length; index++)
          _lineRow(context, index),
        if (_trouble != null) ...[
          const SizedBox(height: 12),
          Text(
            _trouble!,
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.error,
            ),
          ),
        ],
        const SizedBox(height: 16),
        FilledButton.icon(
          onPressed: _matching ? null : _match,
          icon: _matching
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Icon(Icons.compare_arrows),
          label: Text(_matching ? 'Matching' : 'Match it'),
        ),
      ],
    );
  }

  Widget _lineRow(BuildContext context, int index) {
    final theme = Theme.of(context);
    final line = _lines[index];
    final amount = ExactAmount.tryParse(line.amount);
    return ListTile(
      contentPadding: EdgeInsets.zero,
      dense: true,
      leading: Icon(
        line.isIn ? Icons.south_west : Icons.north_east,
        size: 18,
        color: line.isIn ? theme.colorScheme.primary : theme.colorScheme.error,
      ),
      title: Text(
        '${amount?.toDecimalString() ?? line.amount} ${line.isIn ? 'in' : 'out'}',
      ),
      subtitle: Text(
        [
          line.date,
          if (line.reference.trim().isNotEmpty) line.reference.trim(),
          if (line.description.trim().isNotEmpty) line.description.trim(),
        ].join(' · '),
      ),
      trailing: IconButton(
        tooltip: 'Take this line off',
        onPressed: () => setState(() => _lines.removeAt(index)),
        icon: const Icon(Icons.close, size: 18),
      ),
    );
  }

  Widget _result(BuildContext context, Reconciliation answer) {
    final theme = Theme.of(context);
    final statement = ExactAmount.tryParse(answer.statementBalance);
    final book = ExactAmount.tryParse(answer.bookBalance);
    final difference = ExactAmount.tryParse(answer.difference);
    final unexplained = ExactAmount.tryParse(answer.unexplainedDifference);
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
      children: [
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Icon(
                      answer.agrees
                          ? Icons.check_circle_outline
                          : Icons.error_outline,
                      color: answer.agrees
                          ? theme.colorScheme.primary
                          : theme.colorScheme.error,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        answer.accountName.isEmpty
                            ? answer.account
                            : '${answer.account} ${answer.accountName}',
                        style: theme.textTheme.titleMedium,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                Text(answer.summary, style: theme.textTheme.bodyMedium),
                const SizedBox(height: 12),
                _figure(
                  theme,
                  'The statement says',
                  statement,
                  answer.statementBalance,
                ),
                _figure(theme, 'The books say', book, answer.bookBalance),
                _figure(theme, 'Difference', difference, answer.difference),
                _figure(
                  theme,
                  'Left unexplained',
                  unexplained,
                  answer.unexplainedDifference,
                  highlight: !answer.agrees,
                ),
              ],
            ),
          ),
        ),
        if (answer.matched.isNotEmpty)
          _section(context, 'Matched, ${answer.matched.length}', [
            for (final match in answer.matched)
              'Line ${match.line}, ${ExactAmount.tryParse(match.amount)?.toDecimalString() ?? match.amount} '
                  '${match.isIn ? 'in' : 'out'} on ${match.statementDate} is entry '
                  '${match.entryReference.isEmpty ? match.entryDescription : match.entryReference} '
                  'on ${match.entryDate}',
          ]),
        if (answer.onStatementOnly.isNotEmpty)
          _section(
            context,
            'On the statement only, ${answer.onStatementOnly.length}',
            [for (final line in answer.onStatementOnly) _lone(line, true)],
            note:
                'The bank moved this money and your books do not hold it yet.',
          ),
        if (answer.inBooksOnly.isNotEmpty)
          _section(
            context,
            'In your books only, ${answer.inBooksOnly.length}',
            [for (final line in answer.inBooksOnly) _lone(line, false)],
            note: 'Your books hold this and the bank has not shown it yet.',
          ),
      ],
    );
  }

  String _lone(LoneLine line, bool onStatement) {
    final amount =
        ExactAmount.tryParse(line.amount)?.toDecimalString() ?? line.amount;
    final named = line.reference.isNotEmpty
        ? line.reference
        : (line.description.isNotEmpty ? line.description : 'no reference');
    return '$amount ${line.isIn ? 'in' : 'out'} on ${line.date}, $named';
  }

  Widget _figure(
    ThemeData theme,
    String label,
    ExactAmount? figure,
    String raw, {
    bool highlight = false,
  }) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        children: [
          Expanded(
            child: Text(
              label,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
          Text(
            figure?.toDecimalString() ?? raw,
            style: theme.textTheme.bodyMedium?.copyWith(
              color: highlight ? theme.colorScheme.error : null,
              fontWeight: highlight ? FontWeight.w600 : null,
            ),
          ),
        ],
      ),
    );
  }

  Widget _section(
    BuildContext context,
    String title,
    List<String> rows, {
    String? note,
  }) {
    final theme = Theme.of(context);
    return Card(
      margin: const EdgeInsets.only(top: 12),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(title, style: theme.textTheme.titleSmall),
            if (note != null) ...[
              const SizedBox(height: 4),
              Text(
                note,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ],
            const SizedBox(height: 8),
            for (final row in rows)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 3),
                child: Text(row, style: theme.textTheme.bodyMedium),
              ),
          ],
        ),
      ),
    );
  }
}

/// One line of a statement, typed as the bank sent it.
class _LineSheet extends StatefulWidget {
  const _LineSheet();

  @override
  State<_LineSheet> createState() => _LineSheetState();
}

class _LineSheetState extends State<_LineSheet> {
  final _date = TextEditingController();
  final _amount = TextEditingController();
  final _reference = TextEditingController();
  final _description = TextEditingController();
  bool _isIn = true;
  String? _trouble;

  @override
  void dispose() {
    _date.dispose();
    _amount.dispose();
    _reference.dispose();
    _description.dispose();
    super.dispose();
  }

  void _add() {
    final date = _date.text.trim();
    if (!RegExp(r'^\d{4}-\d{2}-\d{2}$').hasMatch(date)) {
      setState(
        () => _trouble = 'Say the day the bank moved it, as 2026-10-05.',
      );
      return;
    }
    final amount = ExactAmount.tryParse(_amount.text.trim());
    if (amount == null || amount.isZero || amount.isNegative) {
      setState(
        () => _trouble = 'Say the amount as the bank sent it, as 500.00.',
      );
      return;
    }
    Navigator.of(context).pop(
      StatementLine(
        date: date,
        amount: _amount.text.trim(),
        isIn: _isIn,
        reference: _reference.text,
        description: _description.text,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: EdgeInsets.fromLTRB(
        16,
        16,
        16,
        16 + MediaQuery.of(context).viewInsets.bottom,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('A line from the statement', style: theme.textTheme.titleMedium),
          const SizedBox(height: 12),
          TextField(
            controller: _date,
            decoration: const InputDecoration(
              labelText: 'Day the bank moved it',
              hintText: '2026-10-05',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _amount,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'Amount',
              hintText: '500.00',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 12),
          SegmentedButton<bool>(
            segments: const [
              ButtonSegment(value: true, label: Text('Money in')),
              ButtonSegment(value: false, label: Text('Money out')),
            ],
            selected: {_isIn},
            onSelectionChanged: (chosen) =>
                setState(() => _isIn = chosen.first),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _reference,
            decoration: const InputDecoration(
              labelText: 'Reference, if the bank gave one',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _description,
            decoration: const InputDecoration(
              labelText: 'What it was, if you know',
              border: OutlineInputBorder(),
            ),
          ),
          if (_trouble != null) ...[
            const SizedBox(height: 10),
            Text(
              _trouble!,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.error,
              ),
            ),
          ],
          const SizedBox(height: 16),
          FilledButton(onPressed: _add, child: const Text('Add this line')),
        ],
      ),
    );
  }
}
