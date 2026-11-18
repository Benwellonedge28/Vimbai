// One invoice: what was billed, what has come in, and recording the next payment.
//
// Recording a payment against an invoice is not the same as posting it: the engine writes no journal entry, and says so in
// its own answer. That distinction is kept on the screen rather than smoothed over, because a Book whose invoices are all
// marked paid and whose income statement has never seen the money is exactly the mistake a business finds out about at
// year end.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';

class InvoiceDetailPage extends StatefulWidget {
  const InvoiceDetailPage({super.key, required this.invoice, this.service});

  final Invoice invoice;

  /// A test passes its own service so the page can be exercised without a server.
  final InvoicingService? service;

  @override
  State<InvoiceDetailPage> createState() => _InvoiceDetailPageState();
}

class _InvoiceDetailPageState extends State<InvoiceDetailPage> {
  late final InvoicingService _invoices =
      widget.service ?? InvoicingService.instance;

  late Invoice _invoice = widget.invoice;
  bool _recording = false;

  Future<void> _recordPayment() async {
    final amount = await showDialog<ExactAmount>(
      context: context,
      builder: (ctx) => _PaymentDialog(outstanding: _invoice.outstanding),
    );
    if (amount == null || !mounted) return;
    setState(() => _recording = true);
    try {
      final result = await _invoices.recordPayment(_invoice, amount);
      if (!mounted) return;
      setState(() {
        _recording = false;
        _invoice = Invoice(
          invoiceNumber: _invoice.invoiceNumber,
          customerId: _invoice.customerId,
          invoiceDate: _invoice.invoiceDate,
          dueDate: _invoice.dueDate,
          total: _invoice.total,
          paid: result.paid,
          status: result.status,
          lines: _invoice.lines,
        );
      });
      _say(
        result.settled
            ? '${_invoice.invoiceNumber} is settled. '
                  'The sale itself posts in Journal entries.'
            : '${result.amount.toDecimalString()} recorded. '
                  '${result.outstanding.toDecimalString()} still to come in.',
      );
    } catch (failure) {
      if (!mounted) return;
      setState(() => _recording = false);
      _say(CouldNotRead.reasonOf(failure));
    }
  }

  void _say(String words) => ScaffoldMessenger.of(
    context,
  ).showSnackBar(SnackBar(content: Text(words)));

  @override
  Widget build(BuildContext context) {
    final owing = _invoice.outstanding;
    return Scaffold(
      appBar: AppBar(title: Text(_invoice.invoiceNumber)),
      floatingActionButton: owing.isZero
          ? null
          : FloatingActionButton.extended(
              onPressed: _recording ? null : _recordPayment,
              icon: const Icon(Icons.payments_outlined),
              label: const Text('Record a payment'),
            ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            _invoice.status == 'paid'
                ? 'Settled'
                : '${owing.toDecimalString()} still to come in',
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: 4),
          Text('Raised ${_invoice.invoiceDate} · due ${_invoice.dueDate}'),
          const SizedBox(height: 16),
          _row('Total', _invoice.total.toDecimalString(), bold: true),
          _row('Paid', _invoice.paid.toDecimalString()),
          _row('Still owing', owing.toDecimalString(), bold: true),
          const Divider(height: 32),
          const Text('Lines', style: TextStyle(fontWeight: FontWeight.bold)),
          const SizedBox(height: 8),
          for (final line in _invoice.lines)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 4),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      '${line.description} · ${line.quantity.toDecimalString()} at '
                      '${line.unitPrice.toDecimalString()}',
                    ),
                  ),
                  Text(line.amount.toDecimalString()),
                ],
              ),
            ),
          const Divider(height: 32),
          Text(
            'Recording a payment here is bookkeeping against the invoice. The money reaching the '
            'ledger is a journal entry in Accounting, which is the only route into it.',
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ],
      ),
    );
  }

  Widget _row(String label, String value, {bool bold = false}) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 2),
    child: Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [
        Text(
          label,
          style: TextStyle(fontWeight: bold ? FontWeight.bold : null),
        ),
        Text(
          value,
          style: TextStyle(fontWeight: bold ? FontWeight.bold : null),
        ),
      ],
    ),
  );
}

/// Asks for a payment, starting from what is still owing.
class _PaymentDialog extends StatefulWidget {
  const _PaymentDialog({required this.outstanding});

  final ExactAmount outstanding;

  @override
  State<_PaymentDialog> createState() => _PaymentDialogState();
}

class _PaymentDialogState extends State<_PaymentDialog> {
  late final TextEditingController _amount = TextEditingController(
    text: widget.outstanding.toDecimalString(),
  );
  String? _complaint;

  @override
  void dispose() {
    _amount.dispose();
    super.dispose();
  }

  void _give() {
    final typed = ExactAmount.tryParse(_amount.text.trim());
    if (typed == null) {
      setState(() => _complaint = 'That is not a figure.');
      return;
    }
    if (typed.isZero) {
      setState(() => _complaint = 'A payment has to be more than nothing.');
      return;
    }
    if (typed > widget.outstanding) {
      setState(
        () => _complaint =
            'That is more than the ${widget.outstanding.toDecimalString()} still owing.',
      );
      return;
    }
    Navigator.of(context).pop(typed);
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Record a payment'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          TextField(
            controller: _amount,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: InputDecoration(
              labelText: 'Amount received',
              helperText:
                  '${widget.outstanding.toDecimalString()} is still owing on this invoice',
            ),
          ),
          if (_complaint != null) ...[
            const SizedBox(height: 8),
            Text(
              _complaint!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ],
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Not now'),
        ),
        ElevatedButton(onPressed: _give, child: const Text('Record it')),
      ],
    );
  }
}
