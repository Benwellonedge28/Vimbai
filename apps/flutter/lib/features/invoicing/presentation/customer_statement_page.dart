// One customer's statement: everything that has passed between them and this Book, in the order it happened.
//
// A statement is not a summary somebody reads for interest - it is the thing a business sends when a customer asks "what do I
// actually owe you". So it is a history: every invoice raised and every payment taken, on its own line with the day, and a
// running balance down the side. An invoice that has since been settled stays on it, because a statement that hid settled
// invoices would not add up to the closing figure it prints.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';

class CustomerStatementPage extends StatefulWidget {
  const CustomerStatementPage({
    super.key,
    required this.customerId,
    this.customerName,
    this.service,
    this.today,
  });

  final String customerId;
  final String? customerName;

  /// A test passes its own service so the page can be exercised without a server.
  final InvoicingService? service;

  /// A test fixes today, so the late figure cannot drift with the calendar.
  final String? today;

  @override
  State<CustomerStatementPage> createState() => _CustomerStatementPageState();
}

class _CustomerStatementPageState extends State<CustomerStatementPage> {
  late final InvoicingService _invoices =
      widget.service ?? InvoicingService.instance;

  late final String _today =
      widget.today ?? DateTime.now().toIso8601String().substring(0, 10);

  bool _loading = true;
  String? _error;
  CustomerStatement? _statement;

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
      final mine = [
        for (final invoice in held)
          if (invoice.customerId == widget.customerId) invoice,
      ];
      // Only this customer's payments: asking for every payment in the Book would be a lot of little requests.
      final payments = <String, List<InvoicePaymentEntry>>{};
      for (final invoice in mine) {
        payments[invoice.invoiceNumber] = await _invoices.paymentEntriesFor(
          invoice.invoiceNumber,
        );
      }
      if (!mounted) return;
      setState(() {
        _statement = customerStatement(
          held,
          payments,
          customerId: widget.customerId,
          today: _today,
          name: widget.customerName,
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
      appBar: AppBar(
        title: Text(_statement?.name ?? widget.customerName ?? 'Statement'),
      ),
      body: _body(),
    );
  }

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'This statement',
        error: _error,
        onRetry: _load,
      );
    }
    final statement = _statement;
    if (statement == null || statement.isEmpty) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(24),
          child: Text(
            'Nothing has passed between you and this customer yet.',
            textAlign: TextAlign.center,
          ),
        ),
      );
    }
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Text(
          '${statement.closing.toDecimalString()} owed as at ${statement.asAt}',
          style: Theme.of(context).textTheme.titleLarge,
        ),
        const SizedBox(height: 4),
        Text(
          '${statement.charged.toDecimalString()} billed, '
          '${statement.paid.toDecimalString()} received, '
          'across ${statement.lines.length} '
          '${statement.lines.length == 1 ? 'movement' : 'movements'}.',
        ),
        if (!statement.late.isZero) ...[
          const SizedBox(height: 8),
          Text(
            '${statement.late.toDecimalString()} of that is past its date, the oldest '
            '${statement.oldestDaysLate} days.',
            style: TextStyle(color: Colors.orange.shade900),
          ),
        ],
        const Divider(height: 32),
        for (final line in statement.lines.reversed) _line(context, line),
        const Divider(height: 24),
        Row(
          children: [
            const Expanded(
              child: Text(
                'Balance',
                style: TextStyle(fontWeight: FontWeight.bold),
              ),
            ),
            Text(
              statement.closing.toDecimalString(),
              style: const TextStyle(fontWeight: FontWeight.bold),
            ),
          ],
        ),
        const SizedBox(height: 16),
        Text(
          'Newest first. The balance beside each line is what was owed just after it, so the '
          'top line is the figure above.',
          style: Theme.of(context).textTheme.bodySmall,
        ),
      ],
    );
  }

  Widget _line(BuildContext context, StatementLine line) {
    final money = line.charge.isZero
        ? '−${line.credit.toDecimalString()}'
        : line.charge.toDecimalString();
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 84,
            child: Text(
              line.date,
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(line.description),
                Text(
                  'balance ${line.running.toDecimalString()}',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          Text(
            money,
            style: TextStyle(
              fontWeight: FontWeight.w600,
              color: line.charge.isZero ? Colors.green.shade800 : null,
            ),
          ),
        ],
      ),
    );
  }
}
