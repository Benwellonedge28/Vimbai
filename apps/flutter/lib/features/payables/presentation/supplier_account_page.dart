// One supplier's account: what was billed, what was paid, and what was left after each.
//
// The mirror of a customer statement, for the other direction. A balance on the "What you owe" page says how much a
// supplier is owed; it does not say how it got there, and that is the question somebody asks before paying: is this
// month's bill on it, did my payment land, why is it higher than I expected.
//
// The engine answers the whole account in one request, including the closing balance, so this page prints what it was told
// rather than adding the lines up itself. A page that summed its own lines could disagree with the balance the Book
// reports, and two figures for one debt is worse than a slow screen.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/payables/data/supplier_account.dart';

class SupplierAccountPage extends StatefulWidget {
  const SupplierAccountPage({
    super.key,
    required this.supplierId,
    this.supplierName,
    this.service,
  });

  final String supplierId;
  final String? supplierName;

  /// A test passes its own service so the page can be exercised without a server.
  final SupplierAccountService? service;

  @override
  State<SupplierAccountPage> createState() => _SupplierAccountPageState();
}

class _SupplierAccountPageState extends State<SupplierAccountPage> {
  late final SupplierAccountService _accounts =
      widget.service ?? SupplierAccountService.instance;

  bool _loading = true;
  String? _error;
  SupplierAccount? _account;

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
      final account = await _accounts.account(widget.supplierId);
      if (!mounted) return;
      setState(() {
        _account = account;
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
        title: Text(_account?.name ?? widget.supplierName ?? 'Supplier'),
      ),
      body: _body(),
    );
  }

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'This supplier account',
        error: _error,
        onRetry: _load,
      );
    }
    final account = _account;
    if (account == null || account.isEmpty) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(24),
          child: Text(
            'Nothing has passed between you and this supplier yet.',
            textAlign: TextAlign.center,
          ),
        ),
      );
    }
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Text(account.headline, style: Theme.of(context).textTheme.titleLarge),
        const SizedBox(height: 4),
        Text(
          '${account.billed.toDecimalString()} billed, '
          '${account.paid.toDecimalString()} paid'
          '${account.credited.isZero ? '' : ', ${account.credited.toDecimalString()} credited back'}, '
          'across ${account.lines.length} '
          '${account.lines.length == 1 ? 'movement' : 'movements'}.',
        ),
        const Divider(height: 32),
        // newest first, so the top line is the figure above
        for (final line in account.lines.reversed) _line(context, line),
        const Divider(height: 24),
        Row(
          children: [
            const Expanded(
              child: Text(
                'Owed now',
                style: TextStyle(fontWeight: FontWeight.bold),
              ),
            ),
            Text(
              account.balance.toDecimalString(),
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

  Widget _line(BuildContext context, SupplierAccountLine line) {
    final adds = line.kind == 'purchase_invoice';
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
                Text(line.wording),
                if (line.reference.isNotEmpty)
                  Text(
                    line.reference,
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                Text(
                  'balance ${line.running.toDecimalString()}',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          Text(
            // a movement the ledger does not count is shown as it is, not dressed up as a payment
            line.moves
                ? (adds
                      ? line.amount.toDecimalString()
                      : '−${line.amount.toDecimalString()}')
                : '—',
            style: TextStyle(
              fontWeight: FontWeight.w600,
              color: line.moves && !adds ? Colors.green.shade800 : null,
            ),
          ),
        ],
      ),
    );
  }
}
