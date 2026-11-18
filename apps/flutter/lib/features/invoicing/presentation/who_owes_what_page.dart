// Who owes what: the register read one way round.
//
// Chasing money is a question about people, not about invoice numbers - "who owes me, and how much" - so the same register
// the engine answered is grouped by customer here. Nothing new is stored: a balance the app kept for itself would be a
// second figure to reconcile against the invoices, and the invoices are the ones that are right.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/ageing_page.dart';

class WhoOwesWhatPage extends StatefulWidget {
  const WhoOwesWhatPage({super.key, this.service});

  /// A test passes its own service so the page can be exercised without a server.
  final InvoicingService? service;

  @override
  State<WhoOwesWhatPage> createState() => _WhoOwesWhatPageState();
}

class _WhoOwesWhatPageState extends State<WhoOwesWhatPage> {
  late final InvoicingService _invoices =
      widget.service ?? InvoicingService.instance;

  bool _loading = true;
  String? _error;
  List<CustomerOwing>? _owing;

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
        _owing = owingByCustomer(
          held,
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
      appBar: AppBar(
        title: const Text('Who owes what'),
        actions: [
          IconButton(
            tooltip: 'How late the money is',
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => AgeingPage(service: widget.service),
              ),
            ),
            icon: const Icon(Icons.hourglass_bottom_outlined),
          ),
        ],
      ),
      body: _body(),
    );
  }

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(what: 'What is owed', error: _error, onRetry: _load);
    }
    final owing = _owing ?? const <CustomerOwing>[];
    if (owing.isEmpty) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(24),
          child: Text(
            'Nobody owes anything on this Book\'s invoices.',
            textAlign: TextAlign.center,
          ),
        ),
      );
    }
    return ListView.separated(
      itemCount: owing.length,
      separatorBuilder: (_, _) => const Divider(height: 1),
      itemBuilder: (context, index) {
        final row = owing[index];
        return ListTile(
          leading: CircleAvatar(child: Text(_initial(row.name))),
          title: Text(row.name),
          subtitle: Text(
            row.invoices == 1
                ? '1 invoice${row.earliestDue.isEmpty ? '' : ', due ${row.earliestDue}'}'
                : '${row.invoices} invoices${row.earliestDue.isEmpty ? '' : ', earliest due ${row.earliestDue}'}',
          ),
          trailing: Text(
            row.owed.toDecimalString(),
            style: const TextStyle(fontWeight: FontWeight.bold),
          ),
        );
      },
    );
  }

  String _initial(String name) =>
      name.trim().isEmpty ? '?' : name.trim().characters.first.toUpperCase();
}
