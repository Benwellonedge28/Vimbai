// What you owe, to whom, and what to settle next.
//
// The mirror of "Who owes you". The figure is what is still owed after the money that actually went out, measured from the
// day each bill fell due - not the bill total, which would show a supplier already paid as though they were still waiting.
//
// Each supplier is one card, led by the sentence naming the bill to settle, because paying is the work and the ageing is
// only how the figure was reached. The bands are the engine's, and the day the ageing was measured from is shown with the
// reason it was used, so nobody has to wonder which day the answer speaks for.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/aged_payables/data/aged_payables_service.dart';

class AgedPayablesPage extends StatefulWidget {
  const AgedPayablesPage({super.key, this.service, this.today});

  /// A test passes its own service so the page can be exercised without a server.
  final AgedPayablesService? service;

  /// The day to age to. Fixed in a test so the page cannot drift with the clock.
  final String? today;

  @override
  State<AgedPayablesPage> createState() => _AgedPayablesPageState();
}

class _AgedPayablesPageState extends State<AgedPayablesPage> {
  late final AgedPayablesService _service =
      widget.service ?? AgedPayablesService.instance;

  AgedPayables? _answer;
  Object? _failure;
  bool _reading = true;

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
      _reading = true;
      _failure = null;
    });
    try {
      final answer = await _service.read(asOf: _today);
      if (!mounted) return;
      setState(() {
        _answer = answer;
        _reading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _failure = error;
        _reading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('What you owe'),
        actions: [
          IconButton(
            tooltip: 'Read it again',
            onPressed: _reading ? null : _load,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: _body(context),
    );
  }

  Widget _body(BuildContext context) {
    if (_reading) return const Center(child: CircularProgressIndicator());
    final failure = _failure;
    if (failure != null) {
      return CouldNotRead(what: 'what you owe', error: failure, onRetry: _load);
    }
    final answer = _answer;
    if (answer == null) return const SizedBox.shrink();
    if (answer.nothingOwed) return _nothingOwed(context, answer);
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
      children: [
        _totals(context, answer),
        for (final supplier in answer.suppliers)
          _supplierCard(context, supplier),
      ],
    );
  }

  Widget _nothingOwed(BuildContext context, AgedPayables answer) {
    final theme = Theme.of(context);
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 24),
      children: [
        Icon(
          Icons.check_circle_outline,
          size: 40,
          color: theme.colorScheme.primary,
        ),
        const SizedBox(height: 12),
        Text('You owe nothing', style: theme.textTheme.titleMedium),
        const SizedBox(height: 6),
        Text(
          answer.summary,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        const SizedBox(height: 12),
        Text(
          'Measured to ${answer.asOf}, which is ${answer.agedFrom}.',
          style: theme.textTheme.bodySmall?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
      ],
    );
  }

  Widget _totals(BuildContext context, AgedPayables answer) {
    final theme = Theme.of(context);
    final owed = ExactAmount.tryParse(answer.owed);
    final late = ExactAmount.tryParse(answer.late);
    return Card(
      margin: const EdgeInsets.only(bottom: 12),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('You owe', style: theme.textTheme.labelMedium),
            const SizedBox(height: 4),
            Text(
              owed?.toDecimalString() ?? answer.owed,
              style: theme.textTheme.headlineSmall,
            ),
            const SizedBox(height: 8),
            Text(
              answer.summary,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            if (late != null && !late.isZero) ...[
              const SizedBox(height: 6),
              Text(
                'Of that, ${late.toDecimalString()} is late'
                '${answer.oldestLateDays > 0 ? ', the oldest ${answer.oldestLateDays} days' : ''}.',
                style: theme.textTheme.bodyMedium,
              ),
            ],
            const SizedBox(height: 10),
            Text(
              'Measured to ${answer.asOf}, which is ${answer.agedFrom}.',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            if (answer.notAged > 0) ...[
              const SizedBox(height: 6),
              Text(
                '${answer.notAged} document${answer.notAged == 1 ? '' : 's'} '
                'carry no readable date, so they were not aged.',
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.error,
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _supplierCard(BuildContext context, OwedToSupplier supplier) {
    final theme = Theme.of(context);
    final owed = ExactAmount.tryParse(supplier.owed);
    return Card(
      margin: const EdgeInsets.only(bottom: 12),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    supplier.name,
                    style: theme.textTheme.titleMedium,
                  ),
                ),
                Text(
                  owed?.toDecimalString() ?? supplier.owed,
                  style: theme.textTheme.titleMedium,
                ),
              ],
            ),
            const SizedBox(height: 8),
            Text(supplier.reminder, style: theme.textTheme.bodyMedium),
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                if (supplier.late != '0.000000')
                  _band(context, 'Late', supplier.late, isLate: true),
                if (supplier.notDue != '0.000000')
                  _band(context, 'Not due yet', supplier.notDue),
                if (supplier.inCredit != '0.000000')
                  _band(context, 'In credit', supplier.inCredit),
              ],
            ),
            const SizedBox(height: 8),
            for (final bill in supplier.bills) _billRow(context, bill),
            const SizedBox(height: 4),
            Align(
              alignment: Alignment.centerRight,
              child: TextButton.icon(
                onPressed: () async {
                  await Clipboard.setData(
                    ClipboardData(text: supplier.reminder),
                  );
                  if (!context.mounted) return;
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(content: Text('The sentence is copied.')),
                  );
                },
                icon: const Icon(Icons.copy_all_outlined, size: 18),
                label: const Text('Copy the sentence'),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _band(
    BuildContext context,
    String label,
    String amount, {
    bool isLate = false,
  }) {
    final theme = Theme.of(context);
    final parsed = ExactAmount.tryParse(amount);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
      decoration: BoxDecoration(
        color: isLate
            ? theme.colorScheme.errorContainer
            : theme.colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Text(
        '$label ${parsed?.toDecimalString() ?? amount}',
        style: theme.textTheme.bodySmall?.copyWith(
          color: isLate
              ? theme.colorScheme.onErrorContainer
              : theme.colorScheme.onSurfaceVariant,
        ),
      ),
    );
  }

  Widget _billRow(BuildContext context, OwedBill bill) {
    final theme = Theme.of(context);
    final outstanding = ExactAmount.tryParse(bill.outstanding);
    final paid = ExactAmount.tryParse(bill.paid);
    final partPaid = paid != null && !paid.isZero;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(bill.number, style: theme.textTheme.bodyMedium),
                Text(
                  bill.isLate
                      ? '${bill.daysLate} days late, due ${bill.dueDate}'
                      : 'Not late yet',
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: bill.isLate
                        ? theme.colorScheme.error
                        : theme.colorScheme.onSurfaceVariant,
                  ),
                ),
                if (partPaid)
                  Text(
                    'Part paid: ${paid.toDecimalString()} of '
                    '${ExactAmount.tryParse(bill.billAmount)?.toDecimalString() ?? bill.billAmount}',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
              ],
            ),
          ),
          Text(
            outstanding?.toDecimalString() ?? bill.outstanding,
            style: theme.textTheme.bodyMedium,
          ),
        ],
      ),
    );
  }
}
