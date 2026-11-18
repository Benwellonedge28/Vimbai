// How late the money is, not just how much of it there is.
//
// "Who owes what" answers how much is outstanding, which is the question a business asks when it is comfortable. The
// question that actually gets chased is how much of it has run past its date, by how long, and whose - so the same register
// is read a second way here, banded by lateness against today.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';

class AgeingPage extends StatefulWidget {
  const AgeingPage({super.key, this.service, this.today});

  /// A test passes its own service so the page can be exercised without a server.
  final InvoicingService? service;

  /// A test fixes today, so a band boundary cannot drift with the calendar.
  final String? today;

  @override
  State<AgeingPage> createState() => _AgeingPageState();
}

class _AgeingPageState extends State<AgeingPage> {
  late final InvoicingService _invoices =
      widget.service ?? InvoicingService.instance;

  late final String _today =
      widget.today ?? DateTime.now().toIso8601String().substring(0, 10);

  bool _loading = true;
  String? _error;
  AgeingReport? _report;

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
      final held = await _invoices.listInvoices();
      final customers = await _invoices.listCustomers();
      if (!mounted) return;
      setState(() {
        _report = ageingReport(
          held,
          today: _today,
          names: {
            for (final customer in customers)
              customer.customerId: customer.label,
          },
        );
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

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('How late the money is')),
      body: _body(),
    );
  }

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'How late the money is',
        error: _error,
        onRetry: _load,
      );
    }
    final report = _report;
    if (report == null || report.isEmpty) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(24),
          child: Text(
            'Nothing is owed, so nothing is late.',
            textAlign: TextAlign.center,
          ),
        ),
      );
    }
    final worst = report.worst;
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Text(
          '${report.late.toDecimalString()} of ${report.owed.toDecimalString()} '
          'is past its due date',
          style: Theme.of(context).textTheme.titleLarge,
        ),
        const SizedBox(height: 4),
        Text('Counted against $_today.'),
        if (worst != null) ...[
          const SizedBox(height: 8),
          Text(
            'The oldest is ${worst.name}, ${worst.oldestDaysLate} days past its date, '
            '${worst.late.toDecimalString()} of it.',
            style: TextStyle(color: Colors.orange.shade900),
          ),
        ],
        const Divider(height: 32),
        for (final band in report.bands)
          if (!band.isEmpty)
            _line(
              band.label,
              '${band.amount.toDecimalString()}  (${band.invoices} '
              '${band.invoices == 1 ? 'invoice' : 'invoices'})',
            ),
        const Divider(height: 32),
        Text('By customer', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        for (final row in report.rows) _customer(context, row),
        const SizedBox(height: 16),
        Text(
          'Late means past the date the invoice was due. Money not yet due is here too, because '
          'a total owed that leaves it out reads smaller than it is.',
          style: Theme.of(context).textTheme.bodySmall,
        ),
      ],
    );
  }

  Widget _customer(BuildContext context, AgeingRow row) {
    final late = [
      for (final band in row.bands.skip(1))
        if (!band.isEmpty)
          '${band.label.replaceAll(' late', '')} '
              '${band.amount.toDecimalString()}',
    ];
    return ListTile(
      contentPadding: EdgeInsets.zero,
      title: Text(row.name),
      subtitle: Text(
        row.late.isZero
            ? 'Nothing late'
            : '${row.late.toDecimalString()} late · ${late.join(' · ')}',
      ),
      trailing: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Text(
            row.owed.toDecimalString(),
            style: const TextStyle(fontWeight: FontWeight.bold),
          ),
          if (row.late.isZero)
            Text('not yet due', style: Theme.of(context).textTheme.bodySmall)
          else
            Text(
              '${row.oldestDaysLate}d',
              style: TextStyle(color: Colors.orange.shade900),
            ),
        ],
      ),
    );
  }

  Widget _line(String label, String value) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 4),
    child: Row(
      children: [
        Expanded(child: Text(label)),
        Text(value, style: const TextStyle(fontWeight: FontWeight.w600)),
      ],
    ),
  );
}
