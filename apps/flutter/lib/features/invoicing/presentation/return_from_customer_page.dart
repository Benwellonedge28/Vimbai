// A customer sends goods back: the sale turned round, and what the business credits them.
//
// This is not a refund of money. The stock comes back, what the customer owes falls, the sale is taken back in a returns
// account, and the tax the invoice carried leaves both the expense it was charged to and the liability it was owed to. No
// bank or cash account is touched, so this screen never asks for one: money actually going back is a cash book entry, and
// recording it here as well would double it.
//
// The credit is capped at what the invoice was billed at when the return names that invoice. A credit bigger than the sale
// would leave the customer owing a negative amount, which reads as the business owing them with nothing to explain it.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/banking/data/currencies.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/sales_return_posting.dart';

class ReturnFromCustomerPage extends StatefulWidget {
  const ReturnFromCustomerPage({
    super.key,
    this.posting,
    this.invoicing,
    this.currencies,
    this.customerId,
    this.customerName,
    this.originalInvoice,
  });

  /// A test passes its own services so the page can be exercised without a server.
  final SalesReturnPostingService? posting;
  final InvoicingService? invoicing;
  final CurrencyService? currencies;

  /// The customer, when the page is opened from their own account.
  final String? customerId;
  final String? customerName;

  /// The invoice being sent back, when it is known.
  final String? originalInvoice;

  @override
  State<ReturnFromCustomerPage> createState() => _ReturnFromCustomerPageState();
}

class _ReturnFromCustomerPageState extends State<ReturnFromCustomerPage> {
  late final SalesReturnPostingService _posting =
      widget.posting ?? SalesReturnPostingService.instance;
  late final InvoicingService _invoicing =
      widget.invoicing ?? InvoicingService.instance;
  late final CurrencyService _currencies =
      widget.currencies ?? CurrencyService.instance;

  final TextEditingController _reference = TextEditingController();
  final TextEditingController _customer = TextEditingController();
  final TextEditingController _total = TextEditingController();
  final TextEditingController _tax = TextEditingController();
  final TextEditingController _rate = TextEditingController();
  final TextEditingController _reason = TextEditingController();
  final TextEditingController _notes = TextEditingController();

  DateTime _returned = DateTime.now();

  /// The invoice this return reverses, when one was picked. Its money and rate are the return's, and its gross is the cap.
  Invoice? _against;

  /// Empty means this Book's own money, which is the ordinary case.
  String _currency = '';

  List<IsoCurrency> _held = const [];
  List<InvoiceCustomer> _customers = const [];
  List<Invoice> _invoices = const [];
  bool _reading = true;
  bool _saving = false;
  String? _complaint;
  String? _readError;

  @override
  void initState() {
    super.initState();
    _customer.text = widget.customerId ?? '';
    _read();
  }

  @override
  void dispose() {
    for (final field in [
      _reference,
      _customer,
      _total,
      _tax,
      _rate,
      _reason,
      _notes,
    ]) {
      field.dispose();
    }
    super.dispose();
  }

  Future<void> _read() async {
    setState(() {
      _reading = true;
      _readError = null;
    });
    try {
      // the customers and the invoices are what the engine already holds; a return names one of each rather than asking
      // somebody to retype an identifier that already exists
      final customers = await _invoicing.listCustomers();
      final invoices = await _invoicing.listInvoices();
      if (!mounted) return;
      setState(() {
        _customers = customers;
        _invoices = invoices;
        _reading = false;
      });
      if (widget.originalInvoice != null)
        _chooseInvoice(widget.originalInvoice!);
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _customers = const [];
        _invoices = const [];
        _reading = false;
        _readError = CouldNotRead.reasonOf(failure);
      });
    }
    try {
      final held = await _currencies.read();
      if (!mounted) return;
      setState(() => _held = held.held);
    } catch (_) {
      // a Book whose currencies could not be read still takes a return in its own money, which is what an empty code means
    }
  }

  /// The invoices this customer was billed, newest first, which is the order somebody picking one wants.
  List<Invoice> get _theirInvoices {
    final who = _customer.text.trim();
    if (who.isEmpty) return const [];
    final theirs = [
      for (final invoice in _invoices)
        if (invoice.customerId == who) invoice,
    ];
    theirs.sort((a, b) => b.invoiceDate.compareTo(a.invoiceDate));
    return theirs;
  }

  /// Taking a return against an invoice carries that invoice's money and rate, so the credit reverses the sale as it stood.
  void _chooseInvoice(String number) {
    final found = _invoices.where((one) => one.invoiceNumber == number);
    if (found.isEmpty) return;
    final invoice = found.first;
    setState(() {
      _against = invoice;
      _currency = invoice.currency.trim().toUpperCase();
      _rate.text = invoice.rate.trim();
    });
  }

  ExactAmount? _figure(TextEditingController field) {
    final typed = field.text.trim();
    if (typed.isEmpty) return null;
    try {
      return ExactAmount.parse(typed);
    } catch (_) {
      return null;
    }
  }

  static String _day(DateTime date) =>
      '${date.year.toString().padLeft(4, '0')}-'
      '${date.month.toString().padLeft(2, '0')}-'
      '${date.day.toString().padLeft(2, '0')}';

  Future<void> _pickDay() async {
    final picked = await showDatePicker(
      context: context,
      initialDate: _returned,
      firstDate: DateTime(2000),
      lastDate: DateTime(2100),
    );
    if (picked == null || !mounted) return;
    setState(() => _returned = picked);
  }

  /// Everything that would stop the engine posting, said before it is asked.
  String? get _refusal {
    if (_customer.text.trim().isEmpty) {
      return 'Say which customer sent the goods back, so what they owe is '
          'reduced against a name rather than nobody.';
    }
    if (_reference.text.trim().isEmpty) {
      return 'Give this return a reference of your own, so the same one cannot '
          'be entered twice by accident.';
    }
    final total = _figure(_total);
    if (total == null || total.isZero || total.isNegative) {
      return 'Say what is being credited back, which is the figure on the '
          'credit note.';
    }
    final tax = _figure(_tax);
    if (tax != null && tax > total) {
      return 'The tax cannot be bigger than the credit it came out of.';
    }
    final against = _against;
    if (against != null && total > against.total) {
      return 'That is more than ${against.invoiceNumber} was billed at '
          '(${against.total.toDecimalString()}). A bigger credit would leave '
          'the customer owing less than nothing.';
    }
    if (_currency.isNotEmpty && (_figure(_rate)?.isZero ?? true)) {
      return 'Say what one $_currency is worth in this Book\'s money, so the '
          'sale comes back out at a figure the books can use.';
    }
    return null;
  }

  Future<void> _post() async {
    final refusal = _refusal;
    if (refusal != null) {
      setState(() => _complaint = refusal);
      return;
    }
    setState(() {
      _saving = true;
      _complaint = null;
    });
    try {
      final posted = await _posting.post(
        entryDate: _day(_returned),
        returnNumber: _reference.text,
        customerId: _customer.text,
        total: _figure(_total)!,
        tax: _figure(_tax),
        originalInvoice: _against?.invoiceNumber ?? '',
        reason: _reason.text,
        currency: _currency,
        rate: _currency.isEmpty ? '' : _rate.text,
        notes: _notes.text,
      );
      if (!mounted) return;
      Navigator.of(context).pop(posted);
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _complaint = CouldNotRead.reasonOf(failure);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final theirInvoices = _theirInvoices;
    return Scaffold(
      appBar: AppBar(title: const Text('Goods back from a customer')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            'The sale is taken back and the customer owes less. No money goes '
            'out with this, so nothing is recorded against the bank.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 16),
          if (_reading) const LinearProgressIndicator(),
          if (_readError != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Text(
                'The customers could not be read, so type the customer\'s id '
                'instead: $_readError',
                style: theme.textTheme.bodySmall,
              ),
            ),
          if (_customers.isNotEmpty)
            DropdownButtonFormField<String>(
              initialValue:
                  _customers.any((one) => one.customerId == _customer.text)
                  ? _customer.text
                  : null,
              decoration: const InputDecoration(
                labelText: 'Customer already on the books',
              ),
              items: [
                for (final customer in _customers)
                  DropdownMenuItem(
                    value: customer.customerId,
                    child: Text(customer.label),
                  ),
              ],
              onChanged: (picked) => setState(() {
                _customer.text = picked ?? '';
                // a return names one customer's invoice, so a different customer means a different invoice
                _against = null;
              }),
            ),
          if (_customers.isNotEmpty) const SizedBox(height: 8),
          TextField(
            controller: _customer,
            decoration: const InputDecoration(
              labelText: 'Customer',
              helperText: 'Who sent the goods back, and who owes less.',
            ),
            onChanged: (_) => setState(() => _against = null),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _reference,
            decoration: const InputDecoration(
              labelText: 'Your reference for this return',
              helperText: 'So the same return cannot be recorded twice.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          if (theirInvoices.isNotEmpty)
            DropdownButtonFormField<String>(
              initialValue:
                  theirInvoices.any(
                    (one) => one.invoiceNumber == _against?.invoiceNumber,
                  )
                  ? _against!.invoiceNumber
                  : null,
              decoration: const InputDecoration(
                labelText: 'The invoice it reverses (if known)',
              ),
              items: [
                for (final invoice in theirInvoices)
                  DropdownMenuItem(
                    value: invoice.invoiceNumber,
                    child: Text(
                      '${invoice.invoiceNumber} · ${invoice.invoiceDate} · '
                      '${invoice.total.toDecimalString()}'
                      '${invoice.currency.isEmpty ? '' : ' ${invoice.currency}'}',
                    ),
                  ),
              ],
              onChanged: (picked) {
                if (picked == null) return;
                _chooseInvoice(picked);
              },
            ),
          if (theirInvoices.isNotEmpty) const SizedBox(height: 8),
          TextField(
            controller: _total,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: InputDecoration(
              labelText: 'What is being credited back',
              helperText: _against == null
                  ? 'The gross figure on the credit note, in the money it was '
                        'raised in.'
                  : 'The gross figure on the credit note. '
                        '${_against!.invoiceNumber} was billed at '
                        '${_against!.total.toDecimalString()}.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _tax,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'Tax inside that figure (if any)',
              helperText:
                  'Comes out of the credit rather than being added to it, so '
                  'the same tax is reversed as was charged.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          DropdownButtonFormField<String>(
            initialValue: _currency,
            decoration: const InputDecoration(
              labelText: 'Which money the credit is in',
            ),
            items: [
              const DropdownMenuItem(
                value: '',
                child: Text('This Book\'s own money'),
              ),
              for (final currency in _held)
                DropdownMenuItem(
                  value: currency.code,
                  child: Text('${currency.code} · ${currency.name}'),
                ),
            ],
            onChanged: (picked) => setState(() => _currency = picked ?? ''),
          ),
          if (_currency.isNotEmpty) ...[
            const SizedBox(height: 8),
            TextField(
              controller: _rate,
              keyboardType: const TextInputType.numberWithOptions(
                decimal: true,
              ),
              decoration: InputDecoration(
                labelText: 'Rate on the day of the return',
                helperText:
                    'How much one $_currency is worth in this Book\'s money. '
                    'The engine converts at this rate and keeps what the '
                    'customer was shown.',
              ),
              onChanged: (_) => setState(() {}),
            ),
          ],
          const SizedBox(height: 8),
          ListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Came back on'),
            subtitle: Text(_day(_returned)),
            trailing: TextButton(
              onPressed: _pickDay,
              child: const Text('Change'),
            ),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _reason,
            decoration: const InputDecoration(
              labelText: 'Why it came back (if any)',
              helperText:
                  'Damaged, wrong item, not what was ordered, and so on.',
            ),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _notes,
            decoration: const InputDecoration(labelText: 'Notes (if any)'),
          ),
          const SizedBox(height: 16),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'What this will write',
                    style: theme.textTheme.titleSmall,
                  ),
                  const SizedBox(height: 4),
                  Text(
                    'Sales returns goes up by the credit, and accounts '
                    'receivable comes down by the same, so what the customer '
                    'owes falls. Any tax inside the credit leaves the expense '
                    'it was charged to and the liability it was owed to. No '
                    'bank or cash account is touched.',
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
            ),
          ),
          if (_complaint != null) ...[
            const SizedBox(height: 12),
            Text(
              _complaint!,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.error,
              ),
            ),
          ],
          const SizedBox(height: 16),
          FilledButton(
            onPressed: _saving ? null : _post,
            child: Text(_saving ? 'Recording…' : 'Record the return'),
          ),
          const SizedBox(height: 24),
        ],
      ),
    );
  }
}
