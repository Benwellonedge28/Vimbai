// Sales invoices: what the Book has billed, and raising the next one.
//
// The engine has held a Book's customers and invoices all along; this is the screen that puts them in somebody's hands.
// Two things are deliberate. A failed read is never shown as "no invoices yet" - an empty register and an unreadable one
// look identical and mean opposite things, so the states are held apart. And the total on a new invoice is summed from its
// lines rather than typed, because the engine refuses a total that disagrees with the lines it is made of.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/invoice_detail_page.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/who_owes_what_page.dart';

class InvoicesPage extends StatefulWidget {
  const InvoicesPage({super.key, this.service});

  /// A test passes its own service so the page can be exercised without a server.
  final InvoicingService? service;

  @override
  State<InvoicesPage> createState() => _InvoicesPageState();
}

class _InvoicesPageState extends State<InvoicesPage> {
  late final InvoicingService _invoices =
      widget.service ?? InvoicingService.instance;

  bool _loading = true;
  String? _error;
  List<Invoice>? _held;
  List<InvoiceCustomer> _customers = const [];

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
      final invoices = await _invoices.listInvoices();
      final customers = await _invoices.listCustomers();
      if (!mounted) return;
      setState(() {
        _held = invoices;
        _customers = customers;
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

  Future<void> _raise() async {
    final raised = await showModalBottomSheet<Invoice>(
      context: context,
      isScrollControlled: true,
      builder: (ctx) => _RaiseInvoiceSheet(
        service: _invoices,
        customers: _customers,
        suggestedNumber: _invoices.nextNumberFor(_held ?? const []),
      ),
    );
    if (raised == null || !mounted) return;
    _say(
      '${raised.invoiceNumber} raised for ${raised.total.toDecimalString()}',
    );
    await _load();
  }

  void _say(String words) => ScaffoldMessenger.of(
    context,
  ).showSnackBar(SnackBar(content: Text(words)));

  String _customerName(String customerId) {
    for (final customer in _customers) {
      if (customer.customerId == customerId) return customer.label;
    }
    return customerId;
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Sales invoices'),
        actions: [
          IconButton(
            tooltip: 'Who owes what',
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute(
                builder: (_) => WhoOwesWhatPage(service: _invoices),
              ),
            ),
            icon: const Icon(Icons.groups_outlined),
          ),
        ],
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _customers.isEmpty && _held != null ? null : _raise,
        icon: const Icon(Icons.receipt_long),
        label: const Text('Raise an invoice'),
      ),
      body: _body(),
    );
  }

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(what: 'The invoices', error: _error, onRetry: _load);
    }
    final held = _held ?? const <Invoice>[];
    if (held.isEmpty) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Icon(Icons.receipt_long, size: 48),
              const SizedBox(height: 12),
              const Text(
                'No invoices raised yet',
                style: TextStyle(fontWeight: FontWeight.bold),
              ),
              const SizedBox(height: 6),
              const Text(
                'An invoice bills a customer for what you sold them. Raising one here does not post to the ledger: '
                'the entry that records the sale is still yours to make in Journal entries.',
                textAlign: TextAlign.center,
              ),
            ],
          ),
        ),
      );
    }
    final owing = ExactAmount.sum(held.map((invoice) => invoice.outstanding));
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
          child: Text(
            owing.isZero
                ? 'Nothing is owing on this Book\'s invoices.'
                : '${owing.toDecimalString()} still to come in across ${held.length} '
                      '${held.length == 1 ? 'invoice' : 'invoices'}.',
            style: Theme.of(context).textTheme.titleSmall,
          ),
        ),
        const Divider(height: 1),
        Expanded(child: _register(held)),
      ],
    );
  }

  Widget _register(List<Invoice> held) {
    return ListView.separated(
      itemCount: held.length,
      separatorBuilder: (_, _) => const Divider(height: 1),
      itemBuilder: (context, index) {
        final invoice = held[index];
        return ListTile(
          onTap: () async {
            await Navigator.of(context).push(
              MaterialPageRoute(
                builder: (_) =>
                    InvoiceDetailPage(invoice: invoice, service: _invoices),
              ),
            );
            if (mounted) await _load();
          },
          leading: CircleAvatar(
            child: Text(invoice.status.characters.first.toUpperCase()),
          ),
          title: Text(
            '${invoice.invoiceNumber} · ${_customerName(invoice.customerId)}',
          ),
          subtitle: Text('${invoice.invoiceDate} · ${_standing(invoice)}'),
          trailing: Text(
            invoice.total.toDecimalString(),
            style: const TextStyle(fontWeight: FontWeight.bold),
          ),
        );
      },
    );
  }

  /// What is left to come in, in words rather than a status code.
  String _standing(Invoice invoice) {
    if (invoice.status == 'paid') return 'paid';
    if (invoice.status == 'void') return 'voided';
    if (invoice.status == 'draft') return 'draft, not yet sent';
    final owing = invoice.outstanding;
    return owing.isZero
        ? invoice.status
        : '${owing.toDecimalString()} still to come in';
  }
}

/// The sheet that raises one invoice: who it bills, what was sold, and when it is due.
class _RaiseInvoiceSheet extends StatefulWidget {
  const _RaiseInvoiceSheet({
    required this.service,
    required this.customers,
    required this.suggestedNumber,
  });

  final InvoicingService service;
  final List<InvoiceCustomer> customers;
  final String suggestedNumber;

  @override
  State<_RaiseInvoiceSheet> createState() => _RaiseInvoiceSheetState();
}

class _RaiseInvoiceSheetState extends State<_RaiseInvoiceSheet> {
  late final TextEditingController _number = TextEditingController(
    text: widget.suggestedNumber,
  );
  final TextEditingController _newCustomer = TextEditingController();
  final List<_LineEntry> _lines = [_LineEntry()];

  InvoiceCustomer? _customer;
  DateTime _raised = DateTime.now();
  DateTime _due = DateTime.now().add(const Duration(days: 30));
  bool _working = false;
  String? _complaint;

  @override
  void initState() {
    super.initState();
    if (widget.customers.isNotEmpty) _customer = widget.customers.first;
  }

  @override
  void dispose() {
    _number.dispose();
    _newCustomer.dispose();
    for (final line in _lines) {
      line.dispose();
    }
    super.dispose();
  }

  ExactAmount get _total => ExactAmount.sum(
    _lines.map((line) => line.amount).whereType<ExactAmount>(),
  );

  Future<void> _pickDate({required bool raised}) async {
    final picked = await showDatePicker(
      context: context,
      initialDate: raised ? _raised : _due,
      firstDate: DateTime(2000),
      lastDate: DateTime(2100),
    );
    if (picked == null) return;
    setState(() {
      if (raised) {
        _raised = picked;
        if (_due.isBefore(_raised)) _due = _raised;
      } else {
        _due = picked;
      }
    });
  }

  Future<void> _raise() async {
    setState(() => _complaint = null);
    final held = _lines
        .map((line) => line.read())
        .whereType<InvoiceLine>()
        .toList();
    if (held.isEmpty) {
      setState(
        () => _complaint =
            'Give at least one line a description, a quantity and a price.',
      );
      return;
    }
    if (_due.isBefore(_raised)) {
      setState(
        () => _complaint = 'The due date cannot be before the invoice date.',
      );
      return;
    }
    setState(() => _working = true);
    try {
      var customer = _customer;
      if (customer == null) {
        final name = _newCustomer.text.trim();
        if (name.isEmpty) {
          throw Exception('Choose a customer, or name a new one.');
        }
        customer = await widget.service.createCustomer(name);
      }
      final invoice = await widget.service.raiseInvoice(
        invoiceNumber: _number.text,
        customer: customer,
        lines: held,
        invoiceDate: _raised,
        dueDate: _due,
      );
      if (!mounted) return;
      Navigator.of(context).pop(invoice);
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _working = false;
        _complaint = CouldNotRead.reasonOf(failure);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(
        left: 16,
        right: 16,
        top: 16,
        bottom: MediaQuery.of(context).viewInsets.bottom + 16,
      ),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text(
              'Raise an invoice',
              style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _number,
              decoration: const InputDecoration(
                labelText: 'Invoice number',
                helperText: 'How the Book will name this invoice',
              ),
            ),
            const SizedBox(height: 12),
            if (widget.customers.isNotEmpty)
              DropdownButtonFormField<InvoiceCustomer>(
                initialValue: _customer,
                decoration: const InputDecoration(labelText: 'Customer'),
                items: [
                  for (final customer in widget.customers)
                    DropdownMenuItem(
                      value: customer,
                      child: Text(customer.label),
                    ),
                ],
                onChanged: (chosen) => setState(() => _customer = chosen),
              ),
            if (widget.customers.isEmpty) ...[
              const Text(
                'This Book has no customers yet, so name the first one here.',
              ),
              TextField(
                controller: _newCustomer,
                decoration: const InputDecoration(labelText: 'Customer name'),
              ),
            ],
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(
                  child: OutlinedButton(
                    onPressed: () => _pickDate(raised: true),
                    child: Text('Raised ${_short(_raised)}'),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: OutlinedButton(
                    onPressed: () => _pickDate(raised: false),
                    child: Text('Due ${_short(_due)}'),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 16),
            const Text('Lines', style: TextStyle(fontWeight: FontWeight.bold)),
            for (var index = 0; index < _lines.length; index++)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Expanded(
                          child: TextField(
                            controller: _lines[index].description,
                            decoration: const InputDecoration(
                              labelText: 'What was sold',
                            ),
                          ),
                        ),
                        if (_lines.length > 1)
                          IconButton(
                            tooltip: 'Remove this line',
                            onPressed: () => setState(() {
                              _lines.removeAt(index).dispose();
                            }),
                            icon: const Icon(Icons.close),
                          ),
                      ],
                    ),
                    Row(
                      children: [
                        Expanded(
                          child: TextField(
                            controller: _lines[index].quantity,
                            keyboardType: const TextInputType.numberWithOptions(
                              decimal: true,
                            ),
                            decoration: const InputDecoration(
                              labelText: 'Quantity',
                            ),
                            onChanged: (_) => setState(() {}),
                          ),
                        ),
                        const SizedBox(width: 8),
                        Expanded(
                          child: TextField(
                            controller: _lines[index].unitPrice,
                            keyboardType: const TextInputType.numberWithOptions(
                              decimal: true,
                            ),
                            decoration: const InputDecoration(
                              labelText: 'Unit price',
                            ),
                            onChanged: (_) => setState(() {}),
                          ),
                        ),
                        const SizedBox(width: 8),
                        SizedBox(
                          width: 88,
                          child: Text(
                            _lines[index].amount?.toDecimalString() ?? '—',
                            textAlign: TextAlign.right,
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            TextButton.icon(
              onPressed: () => setState(() => _lines.add(_LineEntry())),
              icon: const Icon(Icons.add),
              label: const Text('Add a line'),
            ),
            const Divider(),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                const Text(
                  'Total',
                  style: TextStyle(fontWeight: FontWeight.bold),
                ),
                Text(
                  _total.toDecimalString(),
                  style: const TextStyle(fontWeight: FontWeight.bold),
                ),
              ],
            ),
            if (_complaint != null) ...[
              const SizedBox(height: 12),
              Text(
                _complaint!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: _working
                      ? null
                      : () => Navigator.of(context).pop(),
                  child: const Text('Not now'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _working ? null : _raise,
                  child: Text(_working ? 'Raising…' : 'Raise it'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  String _short(DateTime when) =>
      '${when.day.toString().padLeft(2, '0')}/${when.month.toString().padLeft(2, '0')}/${when.year}';
}

/// One line being typed: description, quantity, unit price, and the amount they make.
class _LineEntry {
  final TextEditingController description = TextEditingController();
  final TextEditingController quantity = TextEditingController(text: '1');
  final TextEditingController unitPrice = TextEditingController();

  /// The amount this line makes, or null while it does not yet make one.
  ExactAmount? get amount {
    final count = ExactAmount.tryParse(quantity.text.trim());
    final price = ExactAmount.tryParse(unitPrice.text.trim());
    if (count == null || price == null) return null;
    return count * price;
  }

  /// The line as it will be sent, or null when it is not a line anybody meant.
  InvoiceLine? read() {
    final what = description.text.trim();
    final made = amount;
    if (what.isEmpty || made == null || made.isZero) return null;
    return InvoiceLine(
      description: what,
      quantity: ExactAmount.parse(quantity.text.trim()),
      unitPrice: ExactAmount.parse(unitPrice.text.trim()),
    );
  }

  void dispose() {
    description.dispose();
    quantity.dispose();
    unitPrice.dispose();
  }
}
