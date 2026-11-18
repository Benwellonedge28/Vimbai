// Taking in the money an invoice payment is about.
//
// An invoice payment is bookkeeping against the invoice: the engine lowers what is owed and writes no journal entry, and
// nothing arrives in the cash book. So a business could be owed 455.00, be paid 455.00, and have both records still saying
// the money is somewhere else. This sheet is the step between the two - the invoice's own figure, the account it landed in,
// and the account it clears - recorded as a receipt so the cash book and the invoice finally describe the same event.
//
// Nothing is assumed about where the money went. The account is chosen from the Book's own cash and bank accounts, because
// guessing which bank account a customer paid into is exactly the kind of guess that takes a month to find.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';

class TakePaymentInSheet extends StatefulWidget {
  const TakePaymentInSheet({
    super.key,
    required this.amount,
    required this.invoiceNumber,
    required this.customer, // the customer as the invoice names them
    required this.cash,
    required this.ledger,
    this.onDate,
  });

  final ExactAmount amount;
  final String invoiceNumber;
  final String customer;
  final CashBookService cash;
  final AccountingApiService ledger;

  /// For a test to fix the date; the day it is taken in otherwise.
  final String? onDate;

  static Future<CashEntry?> show(
    BuildContext context, {
    required ExactAmount amount,
    required String invoiceNumber,
    required String customer,
    required CashBookService cash,
    required AccountingApiService ledger,
  }) => showModalBottomSheet<CashEntry>(
    context: context,
    isScrollControlled: true,
    builder: (_) => TakePaymentInSheet(
      amount: amount,
      invoiceNumber: invoiceNumber,
      customer: customer,
      cash: cash,
      ledger: ledger,
    ),
  );

  @override
  State<TakePaymentInSheet> createState() => _TakePaymentInSheetState();
}

class _TakePaymentInSheetState extends State<TakePaymentInSheet> {
  bool _loading = true;
  bool _saving = false;
  String? _error;
  String? _complaint;
  List<CashAccount> _places = const [];
  List<Account> _chart = const [];
  CashAccount? _place;
  Account? _against;

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
      final places = await widget.cash.accounts();
      final chart = await widget.ledger.getAllAccounts();
      if (!mounted) return;
      setState(() {
        _places = [
          for (final account in places)
            if (account.type == 'cash' || account.type == 'bank') account,
        ];
        _chart = chart;
        _place = _places.isEmpty ? null : _places.first;
        _against = _receivable(chart);
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

  /// What the payment clears: the account the sale was billed to.
  Account? _receivable(List<Account> chart) {
    for (final account in chart) {
      final name = account.accountName.toLowerCase();
      if (account.accountType == 'asset' &&
          (name.contains('receivable') || name.contains('debtor'))) {
        return account;
      }
    }
    return null;
  }

  Future<void> _record() async {
    final place = _place;
    final against = _against;
    if (place == null) {
      setState(
        () => _complaint =
            'This Book has no cash or bank account, so there is nowhere for the money to have '
            'landed. Add one in the cash book first.',
      );
      return;
    }
    if (against == null) {
      setState(
        () => _complaint =
            'Choose what this payment clears. The Book has no accounts-receivable account to '
            'propose.',
      );
      return;
    }
    setState(() {
      _saving = true;
      _complaint = null;
    });
    try {
      final entry = await widget.cash.record(
        account: place,
        moneyIn: true,
        amount: widget.amount,
        description: '${widget.invoiceNumber} paid by ${widget.customer}'
            .trim(),
        date:
            widget.onDate ?? DateTime.now().toIso8601String().substring(0, 10),
        againstAccount: against.accountNumber,
      );
      if (!mounted) return;
      Navigator.of(context).pop(entry);
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
    return Padding(
      padding: EdgeInsets.only(
        left: 16,
        right: 16,
        top: 16,
        bottom: MediaQuery.of(context).viewInsets.bottom + 16,
      ),
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Take in ${widget.amount.toDecimalString()}',
              style: Theme.of(context).textTheme.titleLarge,
            ),
            const SizedBox(height: 4),
            Text(
              'Record the money that ${widget.invoiceNumber} was paid with, so the cash book and '
              'the invoice describe the same event.',
            ),
            const SizedBox(height: 12),
            if (_loading) const LinearProgressIndicator(),
            if (_error != null)
              CouldNotRead(
                what: 'Where the money could have landed',
                error: _error,
                onRetry: _load,
              ),
            if (!_loading && _error == null) ...[
              DropdownButtonFormField<String>(
                initialValue: _place?.code,
                isExpanded: true,
                decoration: const InputDecoration(
                  labelText: 'Money arrived in',
                ),
                items: [
                  for (final account in _places)
                    DropdownMenuItem(
                      value: account.code,
                      child: Text(
                        '${account.code} · ${account.name}',
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                ],
                onChanged: (code) {
                  for (final account in _places) {
                    if (account.code == code) {
                      setState(() => _place = account);
                      return;
                    }
                  }
                },
              ),
              const SizedBox(height: 8),
              DropdownButtonFormField<String>(
                initialValue: _against?.accountNumber,
                isExpanded: true,
                decoration: const InputDecoration(
                  labelText: 'What it clears',
                  helperText: 'The account the sale was billed to',
                ),
                items: [
                  for (final account in _chart)
                    DropdownMenuItem(
                      value: account.accountNumber,
                      child: Text(
                        '${account.accountNumber} · ${account.accountName}',
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                ],
                onChanged: (number) {
                  for (final account in _chart) {
                    if (account.accountNumber == number) {
                      setState(() => _against = account);
                      return;
                    }
                  }
                },
              ),
              const SizedBox(height: 8),
              Text(
                'Recorded as money in, posted, and named '
                '"${widget.invoiceNumber} paid by ${widget.customer}". '
                'Putting it into the ledger is the next tap on that entry.',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ],
            if (_complaint != null) ...[
              const SizedBox(height: 8),
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
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Not now'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _saving || _loading ? null : _record,
                  child: Text(_saving ? 'Recording…' : 'Take it in'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
