// Closing the year.
//
// A year's sales and costs have done their work once the year is over. What they came to is the year's profit, and the
// accounts that carried them should not go on carrying them into the next year: a reader of next February's sales wants
// next February's sales, not last year's total sitting behind them.
//
// The screen asks which year and says what closing it does before it is done: every revenue account is emptied into the
// year's profit, every cost account with it, and the difference goes to retained earnings in the owners' equity. It says
// the two things a person needs to know first - that the year's months have to be closed before the year can be, and that
// nothing moves money. It never adds a figure up: the engine states what the year earned, what it spent and what it kept,
// and the screen repeats the engine's answer.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/year_end/data/year_end_posting.dart';

class CloseTheYearPage extends StatefulWidget {
  const CloseTheYearPage({super.key, this.posting});

  /// A test passes its own service so the page can be exercised without a server.
  final YearEndPostingService? posting;

  @override
  State<CloseTheYearPage> createState() => _CloseTheYearPageState();
}

class _CloseTheYearPageState extends State<CloseTheYearPage> {
  late final YearEndPostingService _posting =
      widget.posting ?? YearEndPostingService.instance;

  late final TextEditingController _year = TextEditingController(
    text: '${DateTime.now().year}',
  );

  List<ClosedYear> _closed = const [];
  bool _reading = true;
  bool _closing = false;
  String? _trouble;

  @override
  void initState() {
    super.initState();
    _read();
  }

  @override
  void dispose() {
    _year.dispose();
    super.dispose();
  }

  Future<void> _read() async {
    setState(() {
      _reading = true;
      _trouble = null;
    });
    try {
      final closed = await _posting.read();
      if (!mounted) return;
      setState(() {
        _closed = closed;
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

  /// Everything that would stop the engine closing the year, said before it is asked.
  String? get _refusal {
    final year = _year.text.trim();
    if (!RegExp(r'^\d{4}$').hasMatch(year)) {
      return 'Say which year is being closed, as 2026. A year is four digits.';
    }
    if (_closed.any((one) => one.year == year)) {
      return '$year has already been closed. A correction to a closed year belongs in the year that is open.';
    }
    return null;
  }

  Future<void> _close() async {
    final refusal = _refusal;
    if (refusal != null) {
      setState(() => _trouble = refusal);
      return;
    }
    setState(() {
      _closing = true;
      _trouble = null;
    });
    try {
      final closed = await _posting.close(year: _year.text.trim());
      if (!mounted) return;
      setState(() {
        _closed = [closed, ..._closed];
        _closing = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _closing = false;
        _trouble = CouldNotRead.reasonOf(failure);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Closing the year')),
      body: _reading
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
              children: [
                Text(
                  'Closing the year empties what the year earned and what it spent into one figure and puts that '
                  'figure in the owners\' equity, so the next year starts with nothing behind it.',
                  style: theme.textTheme.bodyMedium,
                ),
                const SizedBox(height: 12),
                Text(
                  'Close the year\'s months first. A month still open could still change what the year came to, so the '
                  'year is closed after its months are.',
                  style: theme.textTheme.bodySmall,
                ),
                const SizedBox(height: 20),
                TextField(
                  controller: _year,
                  keyboardType: TextInputType.number,
                  decoration: const InputDecoration(
                    labelText: 'Which year',
                    helperText: 'Four digits, as 2026.',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) => setState(() {}),
                ),
                const SizedBox(height: 20),
                FilledButton(
                  onPressed: _closing ? null : _close,
                  child: Text(_closing ? 'Closing...' : 'Close the year'),
                ),
                const SizedBox(height: 8),
                Text(
                  'Nothing moves: no money leaves, no account is paid, and the business still holds what it held. '
                  'Only where the year\'s profit sits changes.',
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
                Text('Years already closed', style: theme.textTheme.titleSmall),
                const SizedBox(height: 6),
                if (_closed.isEmpty)
                  Text(
                    'None yet: this Book has not closed a year.',
                    style: theme.textTheme.bodySmall,
                  ),
                for (final one in _closed)
                  ListTile(
                    contentPadding: EdgeInsets.zero,
                    dense: true,
                    title: Text('${one.year}: ${one.whatItKept}'),
                    subtitle: Text(
                      'earned ${one.earned}, spent ${one.spent} · closing entry ${one.entryDate}'
                      '${one.monthsReopenedSince.isEmpty ? '' : ' · reopened since: ${one.monthsReopenedSince.join(', ')}'}',
                    ),
                  ),
              ],
            ),
    );
  }
}
