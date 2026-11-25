// The tax ledger: each tax the Book charges, what it took in, and what has been paid over.
//
// The screen answers the question a business actually asks: how much do I owe on each tax, and what happened to it this
// month. Each tax has its own card, because each is owed to a different place and appears on a different return; the
// movements behind a figure are kept under it, so a total can always be followed to the entry that made it.
//
// The one thing this screen adds to reading is paying over. A tax that was recorded as owed could never be shown as paid,
// so the liability stood for ever; paying it over posts one entry, the tax account giving the liability up and the bank or
// cash it left recording the payment. Nothing here moves money: Vimbai records a payment that happened.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/tax/data/tax_due.dart'
    show monthWindow;
import 'package:vimbai_mobile_client/features/tax/data/tax_ledger.dart';

class TaxLedgerPage extends StatefulWidget {
  const TaxLedgerPage({super.key, this.service, this.today});

  /// A test passes its own service so the page can be exercised without a server.
  final TaxLedgerService? service;

  /// The day to report on. Fixed in a test so the page cannot drift with the clock.
  final String? today;

  @override
  State<TaxLedgerPage> createState() => _TaxLedgerPageState();
}

class _TaxLedgerPageState extends State<TaxLedgerPage> {
  late final TaxLedgerService _service =
      widget.service ?? TaxLedgerService.instance;

  TaxLedger? _ledger;
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
      final ledger = await _service.read(from: window.from, to: window.to);
      if (!mounted) return;
      setState(() {
        _ledger = ledger;
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

  /// Ask for the day and the figure, then pay the tax over.
  Future<void> _payOver(TaxLedgerAccount account) async {
    final window = monthWindow(_today);
    final day = TextEditingController(text: _today);
    final figure = TextEditingController(
      text: account.closing.toDecimalString(),
    );
    String? trouble;
    var saving = false;

    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      builder: (sheet) => StatefulBuilder(
        builder: (sheet, setSheet) {
          Future<void> post() async {
            final typed = figure.text.trim();
            final parsed = ExactAmount.tryParse(typed);
            if (!RegExp(r'^\d{4}-\d{2}-\d{2}$').hasMatch(day.text.trim())) {
              setSheet(
                () => trouble = 'Say the day it was paid, as 2026-11-20.',
              );
              return;
            }
            if (parsed == null) {
              setSheet(
                () => trouble =
                    'That is not a figure the books can carry. Say it as 150.00.',
              );
              return;
            }
            if (parsed.isZero || parsed.isNegative) {
              setSheet(
                () => trouble =
                    'A payment has to be more than nothing: say what was paid.',
              );
              return;
            }
            if (parsed > account.closing) {
              setSheet(
                () => trouble =
                    'That is more than the ${account.closing.toDecimalString()} owed on '
                    '${account.label}. What is taken back is a purchase carrying the tax, '
                    'not a payment.',
              );
              return;
            }
            setSheet(() {
              saving = true;
              trouble = null;
            });
            try {
              final paid = await _service.payOver(
                entryDate: day.text.trim(),
                taxAccountNumber: account.accountNumber,
                amount: typed,
                period: window.from.isEmpty ? '' : window.from.substring(0, 7),
              );
              if (!mounted) return;
              if (sheet.mounted) Navigator.of(sheet).pop();
              ScaffoldMessenger.of(
                context,
              ).showSnackBar(SnackBar(content: Text(paid.reading)));
              await _load();
            } catch (failure) {
              if (!sheet.mounted) return;
              setSheet(() {
                saving = false;
                trouble = CouldNotRead.reasonOf(failure);
              });
            }
          }

          return Padding(
            padding: EdgeInsets.fromLTRB(
              16,
              16,
              16,
              16 + MediaQuery.of(sheet).viewInsets.bottom,
            ),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  'Pay ${account.label} over',
                  style: Theme.of(sheet).textTheme.titleLarge,
                ),
                const SizedBox(height: 6),
                Text(
                  'The books show ${account.closing.toDecimalString()} owed on '
                  '${account.place}. Paying it over takes the liability out of that '
                  'account and out of the bank or cash the money left.',
                ),
                const SizedBox(height: 16),
                TextField(
                  controller: day,
                  decoration: const InputDecoration(
                    labelText: 'The day it was paid',
                    helperText: 'As 2026-11-20: the month follows from it.',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) => setSheet(() {}),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: figure,
                  keyboardType: const TextInputType.numberWithOptions(
                    decimal: true,
                  ),
                  decoration: const InputDecoration(
                    labelText: 'What was paid over',
                    helperText: 'In money, as 150.00.',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) => setSheet(() {}),
                ),
                if (trouble != null) ...[
                  const SizedBox(height: 12),
                  Text(
                    trouble!,
                    style: Theme.of(sheet).textTheme.bodyMedium?.copyWith(
                      color: Theme.of(sheet).colorScheme.error,
                    ),
                  ),
                ],
                const SizedBox(height: 16),
                FilledButton(
                  onPressed: saving ? null : post,
                  child: Text(saving ? 'Paying over...' : 'Pay it over'),
                ),
                const SizedBox(height: 8),
                Text(
                  'Nothing moves here: Vimbai records the payment that happened.',
                  style: Theme.of(sheet).textTheme.bodySmall,
                ),
              ],
            ),
          );
        },
      ),
    );
    day.dispose();
    figure.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Tax ledger'),
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
        what: 'The tax ledger',
        error: failure,
        onRetry: _load,
      );
    }
    if (_reading) return const Center(child: CircularProgressIndicator());
    final ledger = _ledger;
    if (ledger == null) return const Center(child: CircularProgressIndicator());
    final theme = Theme.of(context);

    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
      children: [
        Text(ledger.headline, style: theme.textTheme.titleLarge),
        const SizedBox(height: 6),
        Text(ledger.detail),
        const SizedBox(height: 16),
        if (!ledger.hasTaxAccount)
          Card(
            child: ListTile(
              leading: const Icon(Icons.info_outline),
              title: const Text('Add a tax account'),
              subtitle: const Text(
                'A business that charges tax keeps one. Name it so its name says VAT '
                'or tax, and every taxed sale and purchase will post to it.',
              ),
            ),
          ),
        for (final account in ledger.accounts) _taxCard(account),
        if (ledger.accounts.isEmpty && ledger.hasTaxAccount)
          const Padding(
            padding: EdgeInsets.only(top: 8),
            child: Text(
              'No tax has been posted yet, so there is no account to read.',
            ),
          ),
        const SizedBox(height: 24),
        Text(
          ledger.note.isEmpty
              ? 'Each tax is its own account, so what is owed is never one merged '
                    'figure.'
              : ledger.note,
          style: theme.textTheme.bodySmall,
        ),
      ],
    );
  }

  Widget _taxCard(TaxLedgerAccount account) {
    final theme = Theme.of(context);
    return Card(
      margin: const EdgeInsets.only(bottom: 12),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    account.label,
                    style: theme.textTheme.titleMedium,
                  ),
                ),
                Text(
                  account.owedWords,
                  style: theme.textTheme.titleMedium?.copyWith(
                    color: account.isReclaimable
                        ? theme.colorScheme.primary
                        : null,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 2),
            Text(
              account.rateWords.isEmpty
                  ? account.place
                  : '${account.place} · ${account.rateWords}',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 8),
            Text(account.moved),
            if (!account.opening.isZero)
              Padding(
                padding: const EdgeInsets.only(top: 4),
                child: Text(
                  'Opened on ${account.opening.toDecimalString()}',
                  style: theme.textTheme.bodySmall,
                ),
              ),
            if (account.canBePaidOver)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: OutlinedButton.icon(
                  onPressed: () => _payOver(account),
                  icon: const Icon(Icons.payments_outlined),
                  label: const Text('Pay it over'),
                ),
              ),
            if (account.movements.isNotEmpty)
              ExpansionTile(
                tilePadding: EdgeInsets.zero,
                childrenPadding: const EdgeInsets.only(bottom: 8),
                title: Text('What moved it', style: theme.textTheme.titleSmall),
                children: [
                  for (final movement in account.movements)
                    ListTile(
                      dense: true,
                      contentPadding: EdgeInsets.zero,
                      title: Text(movement.reading),
                      subtitle: movement.description.trim().isEmpty
                          ? null
                          : Text(movement.description),
                      trailing: Text(movement.balance.toDecimalString()),
                    ),
                ],
              ),
          ],
        ),
      ),
    );
  }
}
