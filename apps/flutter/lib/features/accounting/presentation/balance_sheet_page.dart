// The balance sheet, as a person reads it: what the business owns, what it owes, and the owners' stake.
//
// It leads with the answer rather than the table: whether the two sides agree, and when they do not, by exactly how much. A
// section lists only the accounts carrying a balance, because a starter chart's zeros are not information. A Book with
// nothing in it says so instead of claiming a check passed, and a failed read is never shown as an empty sheet.
import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/data/position.dart';

class BalanceSheetPage extends StatefulWidget {
  const BalanceSheetPage({super.key, this.asOf, this.service});

  /// A test fixes the date; the app reads the sheet as it stands today.
  final DateTime? asOf;

  /// A test passes its own service so the page can be exercised without a server.
  final AccountingApiService? service;

  @override
  State<BalanceSheetPage> createState() => _BalanceSheetPageState();
}

class _BalanceSheetPageState extends State<BalanceSheetPage> {
  late final AccountingApiService _api =
      widget.service ?? AccountingApiService();
  late DateTime _asOf = widget.asOf ?? DateTime.now();

  bool _loading = true;
  String? _error;
  Position? _position;

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
      final sheet = await _api.getBalanceSheet(_asOf);
      if (!mounted) return;
      setState(() {
        _position = positionOf(sheet);
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

  Future<void> _pickDate() async {
    final picked = await showDatePicker(
      context: context,
      initialDate: _asOf,
      firstDate: DateTime(2000),
      lastDate: DateTime(2101),
    );
    if (picked == null || !mounted) return;
    setState(() => _asOf = picked);
    await _load();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Balance sheet'),
        actions: [
          IconButton(
            tooltip: 'Read the sheet as at another day',
            onPressed: _pickDate,
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
        what: 'The balance sheet',
        error: _error,
        onRetry: _load,
      );
    }
    final position = _position;
    if (position == null) {
      return const CouldNotRead(
        what: 'The balance sheet',
        error: 'the engine answered nothing',
        onRetry: null,
      );
    }
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
        children: [
          _verdict(position),
          const SizedBox(height: 16),
          _section(position.assets),
          _section(position.liabilities),
          _section(position.equity),
          const SizedBox(height: 12),
          _identity(position),
        ],
      ),
    );
  }

  Widget _verdict(Position position) {
    final theme = Theme.of(context);
    final nothing = position.verdict == PositionVerdict.nothingRecorded;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('What the business is worth', style: theme.textTheme.titleMedium),
        Text(
          'As at ${DateFormat.yMMMd().format(position.asOfDate)}',
          style: theme.textTheme.labelLarge,
        ),
        const SizedBox(height: 6),
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(
              nothing
                  ? Icons.info_outline
                  : (position.verdict == PositionVerdict.balances
                        ? Icons.check_circle_outline
                        : Icons.error_outline),
              color: nothing
                  ? theme.colorScheme.onSurfaceVariant
                  : (position.verdict == PositionVerdict.balances
                        ? Colors.green
                        : theme.colorScheme.error),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(position.headline, style: theme.textTheme.bodyMedium),
            ),
          ],
        ),
      ],
    );
  }

  Widget _section(PositionSection section) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '${section.title} · ${section.total.toDecimalString()}',
            style: theme.textTheme.titleMedium,
          ),
          if (section.isEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(
                section.silent == 0
                    ? 'No ${section.title.toLowerCase()} accounts in the chart.'
                    : 'Nothing in any of the ${section.silent} '
                          '${section.title.toLowerCase()} accounts.',
                style: theme.textTheme.bodySmall,
              ),
            )
          else ...[
            for (final row in section.carrying)
              Padding(
                padding: const EdgeInsets.only(top: 6),
                child: Row(
                  children: [
                    Expanded(child: Text(row.category)),
                    Text(row.amount.toDecimalString()),
                  ],
                ),
              ),
            if (section.silent > 0)
              Padding(
                padding: const EdgeInsets.only(top: 6),
                child: Text(
                  'and ${section.silent} more holding nothing',
                  style: theme.textTheme.bodySmall,
                ),
              ),
          ],
        ],
      ),
    );
  }

  /// The identity, said plainly: what is owned equals what is owed plus what is the owners'.
  Widget _identity(Position position) {
    final theme = Theme.of(context);
    if (position.verdict == PositionVerdict.nothingRecorded)
      return const SizedBox();
    final agrees = position.verdict == PositionVerdict.balances;
    final owed = position.liabilities.total + position.equity.total;
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  'Everything owned',
                  style: theme.textTheme.bodySmall,
                ),
              ),
              Text(position.assets.total.toDecimalString()),
            ],
          ),
          Row(
            children: [
              Expanded(
                child: Text(
                  'Everything owed, plus the owners\' stake',
                  style: theme.textTheme.bodySmall,
                ),
              ),
              Text(owed.toDecimalString()),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            agrees
                ? 'These must agree, and they do. If they ever do not, an entry '
                      'has been posted that the ledger did not balance.'
                : 'These must agree. They differ by ${position.gap.toDecimalString()}, '
                      'which means an entry reached the books without balancing. '
                      '"Books in order" says where to look first.',
            style: theme.textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}
