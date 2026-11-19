// Sales invoices: what the Book has billed, and raising the next one.
//
// The engine has held a Book's customers and invoices all along; this is the screen that puts them in somebody's hands.
// Two things are deliberate. A failed read is never shown as "no invoices yet" - an empty register and an unreadable one
// look identical and mean opposite things, so the states are held apart. And the total on a new invoice is summed from its
// lines rather than typed, because the engine refuses a total that disagrees with the lines it is made of.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/discounts.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/invoice_detail_page.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/who_owes_what_page.dart';

class InvoicesPage extends StatefulWidget {
  const InvoicesPage({super.key, this.service, this.discountService});

  /// A test passes its own service so the page can be exercised without a server.
  final InvoicingService? service;

  /// The discounts a Book gives, so a test can hand the form its own rules.
  final DiscountService? discountService;

  @override
  State<InvoicesPage> createState() => _InvoicesPageState();
}

class _InvoicesPageState extends State<InvoicesPage> {
  late final InvoicingService _invoices =
      widget.service ?? InvoicingService.instance;

  late final DiscountService _discounts =
      widget.discountService ?? DiscountService.instance;

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
        discounts: _discounts,
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
    final owing = owedAcross(held);
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
    required this.discounts,
    required this.customers,
    required this.suggestedNumber,
  });

  final InvoicingService service;

  /// The discounts this Book gives, read when the form opens.
  final DiscountService discounts;
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

  /// The discounts this Book gives, read when the form opens. Empty until read, and left empty if the read fails,
  /// because a discount the engine does not know about would be refused at the point the invoice is raised.
  DiscountTable? _discounts;

  /// The offered rules taken on this invoice. A compulsory rule is not here: the engine gives it without being asked.
  final Set<String> _taken = {};

  /// A figure given off this invoice by hand, off the total before tax.
  final TextEditingController _oneOff = TextEditingController();

  /// The money the customer is billed in, and the rate it was raised at. Empty means the Book's own money.
  final TextEditingController _money = TextEditingController();
  final TextEditingController _rate = TextEditingController();

  /// What the engine says would come off, in its own words. The app never works out a discount itself.
  String? _quote;

  @override
  void initState() {
    super.initState();
    if (widget.customers.isNotEmpty) _customer = widget.customers.first;
    _loadDiscounts();
  }

  Future<void> _loadDiscounts() async {
    try {
      final table = await widget.discounts.read();
      if (!mounted) return;
      setState(() {
        _discounts = table;
      });
      _refreshQuote();
    } catch (_) {
      // a Book whose discounts could not be read still raises invoices; the engine refuses anything it does not hold
    }
  }

  /// Ask the engine what this invoice would carry, so the figure shown is the one that will be posted.
  Future<void> _refreshQuote() async {
    final table = _discounts;
    if (table == null || table.empty) return;
    if (_total.isZero) {
      if (mounted) setState(() => _quote = null);
      return;
    }
    try {
      final asked = [
        for (final one in table.offered)
          if (_taken.contains(one.name)) one.name,
      ];
      final answer = await widget.discounts.quote(
        total: _total.toLedgerString(),
        customerId: _customer?.customerId ?? 'anyone',
        date: _short(_raised),
        rules: asked,
      );
      if (!mounted) return;
      setState(() => _quote = answer.reading);
    } catch (error) {
      if (!mounted) return;
      setState(() => _quote = CouldNotRead.reasonOf(error));
    }
  }

  @override
  void dispose() {
    _number.dispose();
    _newCustomer.dispose();
    _oneOff.dispose();
    _money.dispose();
    _rate.dispose();
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
        discountRules: [
          for (final one in (_discounts?.offered ?? const <BookDiscount>[]))
            if (_taken.contains(one.name)) one.name,
        ],
        discountAmount: _oneOff.text,
        currency: _money.text,
        exchangeRate: _rate.text,
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

  /// What comes off this invoice: the rules the Book gives by itself, the ones offered here, and a figure given by hand.
  Widget _discountSection(BuildContext context) {
    final table = _discounts;
    if (table == null || table.empty) return const SizedBox.shrink();
    final theme = Theme.of(context);
    final automatic = table.automatic;
    final offered = table.offered;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const SizedBox(height: 4),
        const Text('Discount', style: TextStyle(fontWeight: FontWeight.bold)),
        for (final one in automatic)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Text(
              '${one.reading} The engine gives this without being asked.',
              style: theme.textTheme.bodySmall,
            ),
          ),
        for (final one in offered)
          CheckboxListTile(
            contentPadding: EdgeInsets.zero,
            value: _taken.contains(one.name),
            onChanged: (on) {
              setState(() {
                if (on == true) {
                  _taken.add(one.name);
                } else {
                  _taken.remove(one.name);
                }
              });
              _refreshQuote();
            },
            title: Text(one.name),
            subtitle: Text(one.reading),
          ),
        const SizedBox(height: 4),
        TextField(
          controller: _oneOff,
          keyboardType: const TextInputType.numberWithOptions(decimal: true),
          decoration: const InputDecoration(
            labelText: 'A one-off discount',
            helperText: 'Off the total, before tax. Leave empty for none.',
          ),
          onChanged: (_) => _refreshQuote(),
        ),
        if (_quote != null) ...[
          const SizedBox(height: 8),
          Text(_quote!, style: theme.textTheme.bodySmall),
        ],
        const SizedBox(height: 12),
        TextField(
          controller: _money,
          textCapitalization: TextCapitalization.characters,
          decoration: const InputDecoration(
            labelText: 'Billed in another money',
            helperText:
                'A code such as USD. Leave empty when the customer pays in '
                'the money this Book keeps.',
          ),
        ),
        TextField(
          controller: _rate,
          keyboardType: const TextInputType.numberWithOptions(decimal: true),
          decoration: const InputDecoration(
            labelText: 'The rate',
            helperText:
                'What one unit of that money is worth in the Book\'s money. '
                'Needed whenever a money is named.',
          ),
        ),
      ],
    );
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
                onChanged: (chosen) {
                  setState(() => _customer = chosen);
                  _refreshQuote();
                },
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
                            onChanged: (_) {
                              setState(() {});
                              _refreshQuote();
                            },
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
                            onChanged: (_) {
                              setState(() {});
                              _refreshQuote();
                            },
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
            _discountSection(context),
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
