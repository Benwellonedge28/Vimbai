// The cash and bank accounts, with what each of them actually holds.
//
// A business's first question about money is usually the simplest one - how much is there, and where - so this page leads
// with the engine's own cash position rather than a figure added up on the phone. A failed read says it failed: a cash book
// that showed zero because it could not be read would be the worst possible lie in this app.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/cash_account_page.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/new_cash_account_sheet.dart';

class CashBookPage extends StatefulWidget {
  const CashBookPage({super.key, this.service});

  /// A test passes its own service so the page can be exercised without a server.
  final CashBookService? service;

  @override
  State<CashBookPage> createState() => _CashBookPageState();
}

class _CashBookPageState extends State<CashBookPage> {
  late final CashBookService _cash = widget.service ?? CashBookService();

  bool _loading = true;
  String? _error;
  List<CashAccount> _accounts = const [];
  CashPosition? _position;

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
      final accounts = await _cash.accounts();
      final position = await _cash.position();
      if (!mounted) return;
      setState(() {
        _accounts = accounts;
        _position = position;
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
    final made = await NewCashAccountSheet.show(context, _cash);
    if (made != null && mounted) await _load();
  }

  Future<void> _open(CashAccount account) async {
    await Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => CashAccountPage(account: account, service: _cash),
      ),
    );
    if (mounted) await _load();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Cash book')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _add,
        icon: const Icon(Icons.add),
        label: const Text('Add an account'),
      ),
      body: _body(),
    );
  }

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(what: 'The cash book', error: _error, onRetry: _load);
    }
    if (_accounts.isEmpty) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Text(
                'No cash or bank account yet. Add the account the money actually sits in - the till, '
                'the bank, a savings account - and its opening balance, and every receipt and payment '
                'afterwards moves that balance.',
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 16),
              ElevatedButton(
                onPressed: _add,
                child: const Text('Add the first account'),
              ),
            ],
          ),
        ),
      );
    }
    final held = _position;
    return ListView(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                held == null
                    ? 'What each account holds'
                    : '${held.total.toDecimalString()} held in total',
                style: Theme.of(context).textTheme.titleLarge,
              ),
              if (held != null) ...[
                const SizedBox(height: 4),
                Text(
                  [
                    if (!held.cash.isZero)
                      '${held.cash.toDecimalString()} cash',
                    if (!held.bank.isZero)
                      '${held.bank.toDecimalString()} bank',
                    if (!held.savings.isZero)
                      '${held.savings.toDecimalString()} savings',
                  ].join(' · '),
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ],
          ),
        ),
        const Divider(height: 1),
        for (final account in _accounts)
          ListTile(
            leading: Icon(
              account.type == 'cash'
                  ? Icons.payments_outlined
                  : Icons.account_balance_outlined,
            ),
            title: Text('${account.code} · ${account.name}'),
            subtitle: Text(
              [
                account.type,
                if (account.bankName.isNotEmpty) account.bankName,
                account.currency,
              ].join(' · '),
            ),
            trailing: Text(
              account.balance.toDecimalString(),
              style: const TextStyle(fontWeight: FontWeight.bold),
            ),
            onTap: () => _open(account),
          ),
      ],
    );
  }
}
