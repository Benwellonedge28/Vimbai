// What the rate did: the one screen where a business can see the currency story rather than a single number.
//
// Two figures matter and they are not the same news. A difference on something already paid is money the business has; a
// difference on a balance still held is a figure that could move again before anybody is paid. A screen that added them
// into one total would let an owner spend a gain that is only on paper, so this page keeps them apart, says which is which
// in plain words, and lists the entries behind each figure so a total can be followed to the document it came from.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/exchange_differences.dart';

class ExchangeDifferencesPage extends StatefulWidget {
  const ExchangeDifferencesPage({super.key, this.from, this.to, this.service});

  final DateTime? from;
  final DateTime? to;

  /// A test passes its own service so the page can be exercised without a server.
  final ExchangeDifferencesService? service;

  @override
  State<ExchangeDifferencesPage> createState() =>
      _ExchangeDifferencesPageState();
}

class _ExchangeDifferencesPageState extends State<ExchangeDifferencesPage> {
  late final ExchangeDifferencesService _service =
      widget.service ?? ExchangeDifferencesService.instance;

  late DateTime _to = widget.to ?? DateTime.now();
  late DateTime _from = widget.from ?? DateTime(_to.year, _to.month, 1);

  bool _loading = true;
  String? _error;
  ExchangeDifferences? _reading;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final reading = await _service.forPeriod(_from, _to);
      if (!mounted) return;
      setState(() {
        _reading = reading;
        _loading = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _error = CouldNotRead.reasonOf(failure);
        _loading = false;
      });
    }
  }

  Future<void> _pickPeriod() async {
    final picked = await showDateRangePicker(
      context: context,
      firstDate: DateTime(2000),
      lastDate: DateTime(2101),
      initialDateRange: DateTimeRange(start: _from, end: _to),
    );
    if (picked == null || !mounted) return;
    setState(() {
      _from = picked.start;
      _to = picked.end;
    });
    await _load();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('What the rate did'),
        actions: [
          IconButton(
            tooltip: 'Read another period',
            onPressed: _pickPeriod,
            icon: const Icon(Icons.date_range),
          ),
        ],
      ),
      body: _body(),
    );
  }

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'What the rate did',
        error: _error!,
        onRetry: _load,
      );
    }
    final reading = _reading;
    if (reading == null) {
      return CouldNotRead(
        what: 'What the rate did',
        error: 'The reading came back empty.',
        onRetry: _load,
      );
    }
    final theme = Theme.of(context);
    if (reading.isQuiet) {
      return Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Nothing moved on the rate',
              style: theme.textTheme.titleLarge,
            ),
            const SizedBox(height: 8),
            Text(
              'No document in another money was paid or restated between '
              '${reading.from} and ${reading.to}.',
              style: theme.textTheme.bodyMedium,
            ),
          ],
        ),
      );
    }

    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        _Figure(
          title: 'Money in hand',
          explain:
              'Differences on documents that have been paid. This is money '
              'the business has, and it is not sales.',
          figures: reading.settled,
        ),
        const SizedBox(height: 12),
        _Figure(
          title: 'Not money yet',
          explain:
              'Differences on balances still held. The books must show '
              'them, but they can move again before anybody is paid, so they '
              'are not cash to spend.',
          figures: reading.held,
        ),
        const SizedBox(height: 20),
        Text('The entries behind it', style: theme.textTheme.titleMedium),
        const SizedBox(height: 8),
        for (final entry in reading.entries) _EntryRow(entry: entry),
      ],
    );
  }
}

class _Figure extends StatelessWidget {
  const _Figure({
    required this.title,
    required this.explain,
    required this.figures,
  });

  final String title;
  final String explain;
  final GainLoss figures;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final net = figures.net;
    final gained = !net.isNegative && !net.isZero;
    final word = figures.isQuiet
        ? 'Nothing'
        : gained
        ? 'Gain'
        : 'Loss';
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(title, style: theme.textTheme.titleMedium),
            const SizedBox(height: 4),
            Text(
              figures.isQuiet
                  ? 'Nothing'
                  : '$word ${(net.isNegative ? net.negated() : net).toDecimalString()}',
              style: theme.textTheme.headlineSmall,
            ),
            const SizedBox(height: 8),
            Text(
              'Gained ${figures.gain.toDecimalString()}, lost '
              '${figures.loss.toDecimalString()}.',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 8),
            Text(explain, style: theme.textTheme.bodySmall),
          ],
        ),
      ),
    );
  }
}

class _EntryRow extends StatelessWidget {
  const _EntryRow({required this.entry});

  final RateEntry entry;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final gained = !entry.gain.isZero;
    final amount = gained ? entry.gain : entry.loss;
    final which = gained ? 'gained' : 'lost';
    final kind = entry.settled ? 'settled' : 'held';
    return ListTile(
      dense: true,
      contentPadding: EdgeInsets.zero,
      title: Text(entry.reference.isEmpty ? entry.date : entry.reference),
      subtitle: Text(
        '${entry.date} · $which ${amount.toDecimalString()} · $kind',
        style: theme.textTheme.bodySmall,
      ),
    );
  }
}
