// How am I doing: the month so far, read as a decision rather than a report.
//
// Most systems make a person wait for the month to close before they are allowed to see a figure. That is backwards: the
// close is for locking numbers, not for being permitted to look at them. This screen opens on today, reads whatever
// window is chosen, and puts the things a decision actually needs beside the profit:
//
// * pace, compared like for like, so a part month is never set against a whole one;
// * cash against profit, because a profitable month with nothing in the bank is how a good business dies;
// * what is owed each way, and the tax already set aside, so the profit on screen is closer to money that can be spent;
// * and what has not been posted, because a profit that leaves those out is provisional and saying so is better than
//   being found out later.
//
// Nothing is calculated here. The engine answers over the window; this reads it and says it plainly.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/progress/data/measures_reading.dart';
import 'package:vimbai_mobile_client/features/progress/data/progress_reading.dart';

class HowAmIDoingPage extends StatefulWidget {
  const HowAmIDoingPage({super.key, this.service, this.measures, this.today});

  /// A test passes its own service so the page can be exercised without a server.
  final ProgressService? service;

  /// The Book's own figures, which are the ones the screen shows.
  final BookMeasuresService? measures;

  /// The day to report on. Fixed in a test so the page cannot drift with the clock.
  final String? today;

  @override
  State<HowAmIDoingPage> createState() => _HowAmIDoingPageState();
}

class _HowAmIDoingPageState extends State<HowAmIDoingPage> {
  late final ProgressService _service = widget.service ?? ProgressService();
  late final BookMeasuresService _measures =
      widget.measures ?? BookMeasuresService.instance;

  ProgressReading? _reading;
  MeasuresReading? _figures;
  Object? _failure;
  bool _loading = true;

  /// An empty [from] means the month so far, which is the window a person standing in a month means.
  String _from = '';

  @override
  void initState() {
    super.initState();
    _load();
  }

  String get _today {
    final fixed = widget.today;
    if (fixed != null && fixed.isNotEmpty) return fixed;
    final now = DateTime.now();
    final month = now.month.toString().padLeft(2, '0');
    final day = now.day.toString().padLeft(2, '0');
    return '${now.year}-$month-$day';
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _failure = null;
    });
    try {
      // the figures are the Book's own rows and the pace comes from the window comparison, so both are read together
      final answers = await Future.wait([
        _service.read(to: _today, from: _from),
        _measures.read(to: _today, from: _from),
      ]);
      if (!mounted) return;
      setState(() {
        _reading = answers[0] as ProgressReading;
        _figures = answers[1] as MeasuresReading;
        _loading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _failure = error;
        _loading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('How am I doing'),
        actions: [
          IconButton(
            tooltip: 'Read it again',
            onPressed: _loading ? null : _load,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: _body(context),
    );
  }

  Widget _body(BuildContext context) {
    if (_loading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_failure != null) {
      return CouldNotRead(
        what: 'how the business is doing',
        error: _failure,
        onRetry: _load,
      );
    }
    final reading = _reading;
    final figures = _figures;
    if (reading == null) {
      return const Center(child: Text('Nothing has been read yet.'));
    }
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          _windowPicker(context, reading),
          const SizedBox(height: 12),
          if (figures != null) _headline(context, figures, reading),
          const SizedBox(height: 12),
          if (figures != null) _measuresCard(context, figures),
          const SizedBox(height: 12),
          _paceCard(context, reading),
          const SizedBox(height: 12),
          _cashCard(context, reading),
          const SizedBox(height: 12),
          _owedCard(context, reading),
          if (reading.isProvisional) ...[
            const SizedBox(height: 12),
            _provisionalCard(context, reading),
          ],
          const SizedBox(height: 12),
          Text(reading.note, style: Theme.of(context).textTheme.bodySmall),
        ],
      ),
    );
  }

  /// The window, as words a person chose rather than a date picker they must decode.
  Widget _windowPicker(BuildContext context, ProgressReading reading) {
    final choices = <String, String>{
      '': 'This month so far',
      'quarter': 'This quarter',
      'year': 'This year',
    };
    return Wrap(
      spacing: 8,
      children: [
        for (final entry in choices.entries)
          ChoiceChip(
            label: Text(entry.value),
            selected: _from == entry.key,
            onSelected: (_) {
              setState(() => _from = _fromFor(entry.key));
              _load();
            },
          ),
        Chip(label: Text(reading.window)),
      ],
    );
  }

  /// The first day of the quarter or the year that [_today] falls in, so the engine is asked for a real window.
  String _fromFor(String choice) {
    final parts = _today.split('-');
    if (parts.length != 3) return '';
    final year = int.tryParse(parts[0]) ?? 0;
    final month = int.tryParse(parts[1]) ?? 1;
    switch (choice) {
      case 'quarter':
        final firstMonth = ((month - 1) ~/ 3) * 3 + 1;
        return '$year-${firstMonth.toString().padLeft(2, '0')}-01';
      case 'year':
        return '$year-01-01';
      default:
        return '';
    }
  }

  /// The figure the Book leads with, in the Book's own words.
  ///
  /// Which figure leads is a row the owner sets, not a sentence in code: a garage and a clinic lead with different ones.
  Widget _headline(
    BuildContext context,
    MeasuresReading figures,
    ProgressReading reading,
  ) {
    final theme = Theme.of(context);
    final leads = figures.leading;
    if (!reading.hasTraded) {
      return Card(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Nothing has been posted in this window yet',
                style: theme.textTheme.titleMedium,
              ),
              const SizedBox(height: 6),
              Text(
                'A Book with nothing posted has nothing to show. Record what happened and the figures appear here '
                'straight away, without closing anything.',
                style: theme.textTheme.bodyMedium,
              ),
            ],
          ),
        ),
      );
    }
    if (leads.isEmpty) {
      return Card(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Text(
            'This Book has not said which figure leads. Its figures are below, and the one that leads is a row you set.',
            style: theme.textTheme.bodyMedium,
          ),
        ),
      );
    }
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final row in leads.take(2))
              Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: Text(
                  '${row.label}: ${_figure(row)}',
                  style: theme.textTheme.titleMedium,
                ),
              ),
            const SizedBox(height: 6),
            Text(
              figures.rows.isEmpty
                  ? 'A Book with nothing to measure has nothing to show. Record what happened and the figures appear '
                        'straight away, without closing anything.'
                  : 'To ${figures.to}. Nothing is closed to see these; the month is closed to lock them.',
              style: theme.textTheme.bodyMedium,
            ),
          ],
        ),
      ),
    );
  }

  /// One figure as a person reads it: money with its decimals, a share with its sign, a count as a count.
  String _figure(MeasureRow row) {
    if (row.problem != null) return 'could not be read';
    final raw = row.value;
    if (raw == null) return 'not read';
    if (row.isPercent) return _percent(raw);
    if (row.isMoney) return _money(raw);
    return raw;
  }

  /// Every figure the Book measures itself by, in the Book's order, including the ones it could not read.
  Widget _measuresCard(BuildContext context, MeasuresReading figures) {
    final theme = Theme.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'What you measure yourself by',
              style: theme.textTheme.titleSmall,
            ),
            const SizedBox(height: 6),
            for (final row in figures.rows)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 4),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Expanded(
                          child: Text(
                            row.label,
                            style: theme.textTheme.bodyMedium?.copyWith(
                              fontWeight: row.leads ? FontWeight.w600 : null,
                            ),
                          ),
                        ),
                        Text(_figure(row), style: theme.textTheme.bodyMedium),
                      ],
                    ),
                    if (row.problem != null)
                      Text(
                        row.problem!,
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.error,
                        ),
                      ),
                  ],
                ),
              ),
          ],
        ),
      ),
    );
  }

  Widget _paceCard(BuildContext context, ProgressReading reading) {
    final change =
        ExactAmount.tryParse(reading.changeProfit) ?? ExactAmount.zero;
    final ahead = !change.negative;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Against the same days before',
              style: Theme.of(context).textTheme.titleSmall,
            ),
            const SizedBox(height: 6),
            Text(
              '${_money(reading.comparedProfit)} made over ${reading.comparedWindow}. '
              'That is ${_money(reading.changeProfit)} ${ahead ? 'ahead' : 'behind'}.',
              style: Theme.of(context).textTheme.bodyMedium,
            ),
            const SizedBox(height: 4),
            Text(
              'The window is compared like for like: the same number of days, never a part month against a whole one.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        ),
      ),
    );
  }

  Widget _cashCard(BuildContext context, ProgressReading reading) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Money held against money made',
              style: Theme.of(context).textTheme.titleSmall,
            ),
            const SizedBox(height: 6),
            Text(
              '${_money(reading.cashTotal)} in the accounts you keep, against ${_money(reading.profit)} made.',
              style: Theme.of(context).textTheme.bodyMedium,
            ),
            if (reading.madeMoreThanItHolds) ...[
              const SizedBox(height: 6),
              Text(
                'More has been made than is in the bank. That is normal when customers owe you, and it is worth '
                'knowing which it is before the money is spent.',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ],
            for (final account in reading.cashAccounts)
              ListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                title: Text(account.name),
                subtitle: Text(account.kind),
                trailing: Text(_money(account.balance)),
              ),
          ],
        ),
      ),
    );
  }

  Widget _owedCard(BuildContext context, ProgressReading reading) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'What is owed, and what is set aside',
              style: Theme.of(context).textTheme.titleSmall,
            ),
            const SizedBox(height: 6),
            _line(context, 'Owed to you', reading.owedToYou),
            _line(context, 'Owed by you', reading.owedByYou),
            _line(context, 'Tax set aside', reading.taxOwed),
          ],
        ),
      ),
    );
  }

  Widget _line(BuildContext context, String label, String amount) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(label, style: Theme.of(context).textTheme.bodyMedium),
          Text(_money(amount), style: Theme.of(context).textTheme.bodyMedium),
        ],
      ),
    );
  }

  Widget _provisionalCard(BuildContext context, ProgressReading reading) {
    final oldest = reading.oldestDraft;
    return Card(
      color: Theme.of(context).colorScheme.surfaceContainerHighest,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              reading.drafts == 1
                  ? 'One entry in this window is not posted'
                  : '${reading.drafts} entries in this window are not posted',
              style: Theme.of(context).textTheme.titleSmall,
            ),
            const SizedBox(height: 6),
            Text(
              oldest.isEmpty
                  ? 'The figures above are provisional until they are posted.'
                  : 'The oldest is dated $oldest. The figures above are provisional until they are posted, '
                        'because they are what the ledger holds, not what it will hold.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        ),
      ),
    );
  }

  /// A figure as money: two decimal places, and never a silent zero for something that could not be read.
  String _money(String amount) {
    final parsed = ExactAmount.tryParse(amount);
    if (parsed == null) return amount;
    return parsed.toDecimalString();
  }

  /// A margin as a percentage, as the engine stated it.
  String _percent(String amount) {
    final parsed = ExactAmount.tryParse(amount);
    if (parsed == null) return '$amount%';
    return '${parsed.toDecimalString()}%';
  }
}
