// Everyone you bill: the customer register itself, with the two things a register is for.
//
// First, knowing who is on the books: every customer, whether or not they owe anything, with the way to reach them and the
// last time they were billed. Second, keeping that list clean: a name can be corrected, and a customer who has never been
// billed can be taken off. A customer with invoices is refused removal here, before anything is sent, because the engine
// would delete them and leave their invoices pointing at nobody.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/customer_register.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/customer_statement_page.dart';

class CustomerRegisterPage extends StatefulWidget {
  const CustomerRegisterPage({super.key, this.service});

  /// A test passes its own service so the page can be exercised without a server.
  final InvoicingService? service;

  @override
  State<CustomerRegisterPage> createState() => _CustomerRegisterPageState();
}

class _CustomerRegisterPageState extends State<CustomerRegisterPage> {
  late final InvoicingService _invoices =
      widget.service ?? InvoicingService.instance;

  bool _loading = true;
  String? _error;
  List<CustomerOnTheBooks>? _register;

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
      final customers = await _invoices.listCustomers();
      final invoices = await _invoices.listInvoices();
      if (!mounted) return;
      setState(() {
        _register = registerOf(customers, invoices);
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

  Future<void> _add() async {
    final register = _register ?? const <CustomerOnTheBooks>[];
    final details = await showDialog<_CustomerDetails>(
      context: context,
      builder: (_) => _CustomerSheet(existing: register),
    );
    if (details == null || !mounted) return;
    try {
      await _invoices.createCustomer(details.name);
      await _load();
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('${details.name} is on the books.')),
      );
    } catch (failure) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(CouldNotRead.reasonOf(failure))));
    }
  }

  Future<void> _change(CustomerOnTheBooks row) async {
    final register = _register ?? const <CustomerOnTheBooks>[];
    final details = await showDialog<_CustomerDetails>(
      context: context,
      builder: (_) => _CustomerSheet(existing: register, editing: row),
    );
    if (details == null || !mounted) return;
    try {
      await _invoices.updateCustomer(
        row.customer.customerId,
        name: details.name,
        email: details.email,
        phone: details.phone,
        address: details.address,
      );
      await _load();
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('${details.name} is updated.')));
    } catch (failure) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(CouldNotRead.reasonOf(failure))));
    }
  }

  Future<void> _remove(CustomerOnTheBooks row) async {
    final refusal = row.removalRefusal;
    if (refusal != null) {
      await _say('Cannot remove ${row.name}', refusal);
      return;
    }
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Remove ${row.name}?'),
        content: const Text(
          'They have never been billed, so nothing is lost. Any invoice raised '
          'against them later would name a new customer.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Keep them'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Remove'),
          ),
        ],
      ),
    );
    if (sure != true || !mounted) return;
    try {
      await _invoices.removeCustomer(row.customer.customerId);
      await _load();
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('${row.name} is off the books.')));
    } catch (failure) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(CouldNotRead.reasonOf(failure))));
    }
  }

  Future<void> _say(String title, String detail) => showDialog<void>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(title),
      content: Text(detail),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Close'),
        ),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Everyone you bill'),
        actions: [
          IconButton(
            tooltip: 'Add a customer',
            onPressed: _loading ? null : _add,
            icon: const Icon(Icons.person_add_alt),
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
        what: 'Your customers',
        error: _error,
        onRetry: _load,
      );
    }
    final register = _register ?? const <CustomerOnTheBooks>[];
    if (register.isEmpty) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Icon(Icons.people_outline, size: 48),
              const SizedBox(height: 12),
              const Text(
                'Nobody is on the books yet.',
                style: TextStyle(fontWeight: FontWeight.bold),
              ),
              const SizedBox(height: 6),
              const Text(
                'A customer is added here, or by naming one while raising an '
                'invoice.',
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 12),
              FilledButton(
                onPressed: _add,
                child: const Text('Add a customer'),
              ),
            ],
          ),
        ),
      );
    }

    final owing = register.where((row) => !row.owed.isZero).length;
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView.separated(
        itemCount: register.length + 1,
        separatorBuilder: (_, _) => const Divider(height: 1),
        itemBuilder: (context, index) {
          if (index == 0) return _heading(register, owing);
          final row = register[index - 1];
          return ListTile(
            title: Text(row.name),
            subtitle: Text(_under(row)),
            trailing: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  row.owed.isZero
                      ? 'nothing owing'
                      : row.owed.toDecimalString(),
                  style: TextStyle(
                    fontWeight: row.owed.isZero
                        ? FontWeight.normal
                        : FontWeight.bold,
                  ),
                ),
                PopupMenuButton<String>(
                  tooltip: 'More for ${row.name}',
                  onSelected: (choice) {
                    if (choice == 'change') _change(row);
                    if (choice == 'remove') _remove(row);
                  },
                  itemBuilder: (context) => const [
                    PopupMenuItem(
                      value: 'change',
                      child: Text('Change details'),
                    ),
                    PopupMenuItem(value: 'remove', child: Text('Remove')),
                  ],
                ),
              ],
            ),
            onTap: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => CustomerStatementPage(
                  customerId: row.customer.customerId,
                  customerName: row.name,
                  service: widget.service,
                ),
              ),
            ),
          );
        },
      ),
    );
  }

  /// The line under a name: how to reach them, and the last time they were billed.
  String _under(CustomerOnTheBooks row) {
    final parts = <String>[];
    if (row.customer.reachable) {
      parts.add(
        row.customer.email.isNotEmpty ? row.customer.email : row.customer.phone,
      );
    }
    if (row.neverBilled) {
      parts.add('never billed');
    } else {
      parts.add(
        '${row.invoices} ${row.invoices == 1 ? 'invoice' : 'invoices'}'
        '${row.lastBilledOn.isEmpty ? '' : ' · last on ${row.lastBilledOn}'}',
      );
    }
    return parts.join(' · ');
  }

  Widget _heading(List<CustomerOnTheBooks> register, int owing) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '${register.length} ${register.length == 1 ? 'customer' : 'customers'}'
            '${owing == 0 ? ', none owing' : ', $owing owing'}',
            style: theme.textTheme.titleMedium,
          ),
          const SizedBox(height: 4),
          Text(
            'Tap a customer for their account. A name can be corrected here; a '
            'customer with invoices stays, because their invoices point at them.',
            style: theme.textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}

/// What the sheet hands back.
class _CustomerDetails {
  const _CustomerDetails({
    required this.name,
    this.email = '',
    this.phone = '',
    this.address = '',
  });

  final String name;
  final String email;
  final String phone;
  final String address;
}

/// Add a customer, or correct one.
///
/// A name already on the books is refused with the customer who holds it named: two spellings of one customer is exactly
/// what a register exists to stop, and the person is one tap from fixing the right one.
class _CustomerSheet extends StatefulWidget {
  const _CustomerSheet({required this.existing, this.editing});

  final List<CustomerOnTheBooks> existing;
  final CustomerOnTheBooks? editing;

  @override
  State<_CustomerSheet> createState() => _CustomerSheetState();
}

class _CustomerSheetState extends State<_CustomerSheet> {
  late final _name = TextEditingController(text: widget.editing?.name ?? '');
  late final _email = TextEditingController(
    text: widget.editing?.customer.email ?? '',
  );
  late final _phone = TextEditingController(
    text: widget.editing?.customer.phone ?? '',
  );
  late final _address = TextEditingController(
    text: widget.editing?.customer.address ?? '',
  );
  String? _complaint;

  @override
  void dispose() {
    _name.dispose();
    _email.dispose();
    _phone.dispose();
    _address.dispose();
    super.dispose();
  }

  void _submit() {
    final wanted = _name.text.trim();
    if (wanted.isEmpty) {
      setState(() => _complaint = 'A customer needs a name.');
      return;
    }
    final taken = nameAlreadyTaken(
      wanted,
      widget.existing.where((row) => row != widget.editing).toList(),
    );
    if (taken != null) {
      setState(
        () => _complaint =
            '${taken.name} is already on the books. Open them to change their '
            'details instead of adding a second one.',
      );
      return;
    }
    Navigator.of(context).pop(
      _CustomerDetails(
        name: wanted,
        email: _email.text.trim(),
        phone: _phone.text.trim(),
        address: _address.text.trim(),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final editing = widget.editing != null;
    return AlertDialog(
      title: Text(editing ? 'Change details' : 'Add a customer'),
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: _name,
              decoration: const InputDecoration(labelText: 'Name'),
              textCapitalization: TextCapitalization.words,
            ),
            TextField(
              controller: _email,
              decoration: const InputDecoration(labelText: 'Email (optional)'),
              keyboardType: TextInputType.emailAddress,
            ),
            TextField(
              controller: _phone,
              decoration: const InputDecoration(labelText: 'Phone (optional)'),
              keyboardType: TextInputType.phone,
            ),
            TextField(
              controller: _address,
              decoration: const InputDecoration(
                labelText: 'Address (optional)',
              ),
              maxLines: 2,
            ),
            if (_complaint != null) ...[
              const SizedBox(height: 12),
              Text(
                _complaint!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: _submit, child: Text(editing ? 'Save' : 'Add')),
      ],
    );
  }
}
