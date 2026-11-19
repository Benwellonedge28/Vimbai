// What you owe the taxman, read from the tax the Book has actually posted.
//
// The screen answers one question in the order a person asks it: how much is due, what made it up, and which documents
// are behind it. It never applies a rate, so a figure here can always be followed to an invoice or a bill.
//
// A Book with no VAT account is told what to add rather than shown a confident nil: a nil figure with no account behind it
// is the most expensive kind of wrong, because it looks like good news.
import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/tax/data/tax_due.dart';

class WhatYouOwePage extends StatefulWidget {
  const WhatYouOwePage({super.key, this.service, this.today});

  final TaxDueService? service;

  /// The day to report on. Fixed in a test so the page cannot drift with the clock.
  final String? today;

  @override
  State<WhatYouOwePage> createState() => _WhatYouOwePageState();
}

class _WhatYouOwePageState extends State<WhatYouOwePage> {
  late final TaxDueService _service = widget.service ?? TaxDueService.instance;

  TaxDue? _tax;
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
    final window = monthWindow(_today);
    try {
      final tax = await _service.read(from: window.from, to: window.to);
      if (!mounted) return;
      setState(() {
        _tax = tax;
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
        title: const Text('What you owe the taxman'),
        actions: [
          IconButton(
            onPressed: _reading ? null : _load,
            icon: const Icon(Icons.refresh),
            tooltip: 'Read again',
          ),
        ],
      ),
      body: _body(),
    );
  }

  Widget _body() {
    final failure = _failure;
    if (failure != null) {
      return CouldNotRead(
        what: 'What the tax account holds',
        error: failure,
        onRetry: _load,
      );
    }
    if (_reading) {
      return const Center(child: CircularProgressIndicator());
    }
    final tax = _tax;
    if (tax == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Text(tax.headline, style: Theme.of(context).textTheme.titleLarge),
        const SizedBox(height: 6),
        Text(tax.detail),
        const SizedBox(height: 16),
        if (!tax.hasTaxAccount)
          Card(
            child: ListTile(
              leading: const Icon(Icons.info_outline),
              title: const Text('Add a VAT account'),
              subtitle: const Text(
                'A business that charges VAT keeps one. Name it so its name says VAT, '
                'and every taxed sale and purchase will post to it.',
              ),
            ),
          ),
        if (tax.accounts.isNotEmpty) ...[
          Text('Where it sits', style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          for (final account in tax.accounts)
            Card(
              child: ListTile(
                leading: const Icon(Icons.account_balance_wallet_outlined),
                title: Text(account.label),
                subtitle: Text(
                  '${account.collected.toDecimalString()} collected, '
                  '${account.reclaimable.toDecimalString()} reclaimed',
                ),
              ),
            ),
          const SizedBox(height: 16),
        ],
        Text('What made it up', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        if (tax.nothingTaxed)
          const Text(
            'No sale or purchase in this month carried tax, so there is nothing to '
            'follow the figure back to.',
          ),
        for (final document in tax.documents)
          Card(
            child: ListTile(
              leading: Icon(
                document.isSale
                    ? Icons.receipt_long
                    : Icons.shopping_bag_outlined,
              ),
              title: Text(document.reading),
              subtitle: Text(
                'On a total of ${document.total.toDecimalString()}',
              ),
              trailing: Text(
                '${document.isSale ? '+' : '-'}${document.tax.toDecimalString()}',
                style: Theme.of(context).textTheme.titleSmall,
              ),
            ),
          ),
        const SizedBox(height: 24),
        const Text(
          'The figure is the tax the Book has posted, not a rate applied to a total. A '
          'sale posts its tax to the VAT account as money owed, and a purchase posts its '
          'tax there as money to reclaim.',
        ),
      ],
    );
  }
}
