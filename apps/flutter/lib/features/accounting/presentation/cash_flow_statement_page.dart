// Where the money went, as a person reads it: did cash grow or fall, by how much, and what for.
//
// It leads with the movement and the two balances, not with a table of every line. The three classes follow, each with the
// sources that actually moved money, largest first. Two things are said only when they are true: that the cash book and the
// books disagree about the same money, and that the engine met an account it could not classify. A period with no movement
// says so rather than showing three empty headings, and a failed read is never drawn as a quiet period.
import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/data/cash_movement.dart';

class CashFlowStatementPage extends StatefulWidget {
  const CashFlowStatementPage({super.key, this.start, this.end, this.service});

  /// A test fixes the period; the app reads the last three months.
  final DateTime? start;
  final DateTime? end;

  /// A test passes its own service so the page can be exercised without a server.
  final AccountingApiService? service;

  @override
  State<CashFlowStatementPage> createState() => _CashFlowStatementPageState();
}

class _CashFlowStatementPageState extends State<CashFlowStatementPage> {
  late final AccountingApiService _api =
      widget.service ?? AccountingApiService();
  late DateTime _end = widget.end ?? DateTime.now();
  late DateTime _start =
      widget.start ?? DateTime(_end.year, _end.month - 3, _end.day);

  bool _loading = true;
  String? _error;
  CashMovement? _movement;

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
      final movement = await _api.getCashMovement(_start, _end);
      if (!mounted) return;
      setState(() {
        _movement = movement;
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
      initialDateRange: DateTimeRange(start: _start, end: _end),
    );
    if (picked == null || !mounted) return;
    setState(() {
      _start = picked.start;
      _end = picked.end;
    });
    await _load();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Cash flow'),
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
        what: 'Where the money went',
        error: _error,
        onRetry: _load,
      );
    }
    final movement = _movement;
    if (movement == null) {
      return const CouldNotRead(
        what: 'Where the money went',
        error: 'the engine answered nothing',
        onRetry: null,
      );
    }
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
        children: [
          _verdict(movement),
          const SizedBox(height: 16),
          if (movement.verdict == MovementVerdict.nothingMoved)
            _quietPeriod(movement)
          else
            for (final section in movement.sections) _section(section),
          if (movement.verdict != MovementVerdict.nothingMoved) ...[
            const SizedBox(height: 8),
            _closing(movement),
          ],
          if (movement.cashBookNote != null) _note(movement.cashBookNote!),
          if (movement.unclassifiedNote != null)
            _note(movement.unclassifiedNote!),
        ],
      ),
    );
  }

  Widget _verdict(CashMovement movement) {
    final theme = Theme.of(context);
    final nothing = movement.verdict == MovementVerdict.nothingMoved;
    final icon = switch (movement.verdict) {
      MovementVerdict.grew => Icons.trending_up,
      MovementVerdict.shrank => Icons.trending_down,
      MovementVerdict.stoodStill => Icons.drag_handle,
      MovementVerdict.nothingMoved => Icons.info_outline,
    };
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('Where the money went', style: theme.textTheme.titleMedium),
        Text(
          '${DateFormat.yMMMd().format(movement.start)} to '
          '${DateFormat.yMMMd().format(movement.end)}',
          style: theme.textTheme.labelLarge,
        ),
        const SizedBox(height: 6),
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(
              icon,
              color: nothing
                  ? theme.colorScheme.onSurfaceVariant
                  : (movement.verdict == MovementVerdict.grew
                        ? Colors.green
                        : theme.colorScheme.onSurfaceVariant),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(movement.headline, style: theme.textTheme.bodyLarge),
            ),
          ],
        ),
      ],
    );
  }

  Widget _quietPeriod(CashMovement movement) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Text(
          'Receipts and payments through the cash and bank accounts will '
          'appear here as they are posted. Entries still in draft move '
          'nothing until they are posted.',
          style: Theme.of(context).textTheme.bodyMedium,
        ),
      ),
    );
  }

  Widget _section(CashSection section) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(section.title, style: theme.textTheme.titleSmall),
              Text(
                section.net.toDecimalString(),
                style: theme.textTheme.titleSmall,
              ),
            ],
          ),
          if (section.isEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(
                'Nothing moved here.',
                style: theme.textTheme.bodySmall,
              ),
            )
          else
            for (final source in section.sources)
              Padding(
                padding: const EdgeInsets.only(left: 8, top: 4),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Expanded(
                      child: Text(
                        source.description,
                        style: theme.textTheme.bodyMedium,
                      ),
                    ),
                    Text(
                      source.amount.toDecimalString(),
                      style: theme.textTheme.bodyMedium,
                    ),
                  ],
                ),
              ),
          if (section.quiet > 0)
            Padding(
              padding: const EdgeInsets.only(left: 8, top: 4),
              child: Text(
                '${section.quiet} other '
                '${section.quiet == 1 ? 'account' : 'accounts'} moved nothing',
                style: theme.textTheme.bodySmall,
              ),
            ),
          const Divider(height: 20),
        ],
      ),
    );
  }

  Widget _closing(CashMovement movement) {
    final theme = Theme.of(context);
    Widget row(String label, ExactAmount amount, {bool strong = false}) =>
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 2),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(
                label,
                style: strong
                    ? theme.textTheme.titleSmall
                    : theme.textTheme.bodyMedium,
              ),
              Text(
                amount.toDecimalString(),
                style: strong
                    ? theme.textTheme.titleSmall
                    : theme.textTheme.bodyMedium,
              ),
            ],
          ),
        );
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          children: [
            row('Held at the start', movement.opening),
            row('Moved in this period', movement.netChange),
            const Divider(),
            row('Held at the end', movement.closing, strong: true),
          ],
        ),
      ),
    );
  }

  Widget _note(String note) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(
            Icons.info_outline,
            size: 18,
            color: theme.colorScheme.onSurfaceVariant,
          ),
          const SizedBox(width: 8),
          Expanded(child: Text(note, style: theme.textTheme.bodySmall)),
        ],
      ),
    );
  }
}
