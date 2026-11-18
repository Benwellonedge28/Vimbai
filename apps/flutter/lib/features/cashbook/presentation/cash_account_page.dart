// One cash or bank account: what has moved through it, and recording the next receipt or payment.
//
// Money in and money out are one screen with one switch, because that is the only thing that differs between them - the same
// date, the same figure, the same voucher - and two screens would be two places for the same entry. What the engine does
// with the answer is not hidden: it moves the account's balance as it writes the entry, so the balance shown afterwards is
// the engine's, read back rather than assumed.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/post_cash_entry_sheet.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/reconcile_sheet.dart';

class CashAccountPage extends StatefulWidget {
  const CashAccountPage({
    super.key,
    required this.account,
    this.service,
    this.accounting,
  });

  final CashAccount account;

  /// A test passes its own service so the page can be exercised without a server.
  final CashBookService? service;

  /// The ledger, for putting an entry into the books. Injectable for the same reason.
  final AccountingApiService? accounting;

  @override
  State<CashAccountPage> createState() => _CashAccountPageState();
}

class _CashAccountPageState extends State<CashAccountPage> {
  late final CashBookService _cash = widget.service ?? CashBookService();
  late final AccountingApiService _ledger =
      widget.accounting ?? AccountingApiService();

  /// The entries this visit has put into the ledger, so the screen does not invite the same posting twice.
  final Set<String> _posted = {};

  late CashAccount _account = widget.account;
  bool _loading = true;
  String? _error;
  List<CashEntry> _entries = const [];
  List<CashReconciliation> _checks = const [];

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
      final entries = await _cash.entries(_account.code);
      final accounts = await _cash.accounts();
      final checks = await _cash.reconciliations(_account.code);
      if (!mounted) return;
      setState(() {
        _entries = entries;
        _checks = checks;
        for (final fresh in accounts) {
          if (fresh.code == _account.code) _account = fresh;
        }
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

  Future<void> _record({required bool moneyIn}) async {
    final made = await showModalBottomSheet<bool>(
      context: context,
      isScrollControlled: true,
      builder: (_) =>
          _EntrySheet(service: _cash, account: _account, moneyIn: moneyIn),
    );
    if (made == true && mounted) await _load();
  }

  Future<void> _reconcile() async {
    final made = await ReconcileSheet.show(
      context,
      account: _account,
      service: _cash,
    );
    if (made != null && mounted) {
      await _load();
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            made.agrees
                ? 'Recorded. The books and the bank agree.'
                : 'Recorded with the gap as it stands today.',
          ),
        ),
      );
    }
  }

  Future<void> _markChecked(CashReconciliation check) async {
    final who = await showDialog<String>(
      context: context,
      builder: (_) => const _ReviewerDialog(),
    );
    if (who == null || !mounted) return;
    try {
      await _cash.markReconciled(
        check,
        reviewedBy: who,
        completedAt: DateTime.now().toIso8601String(),
      );
      if (!mounted) return;
      await _load();
    } catch (failure) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(CouldNotRead.reasonOf(failure))));
    }
  }

  Future<void> _post(CashEntry entry) async {
    final posted = await PostCashEntrySheet.show(
      context,
      entry: entry,
      account: _account,
      ledger: _ledger,
    );
    if (posted == true && mounted) {
      setState(() => _posted.add(entry.id));
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('The entry is in the ledger.')),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final moneyIn = ExactAmount.sum(
      _entries.where((entry) => entry.moneyIn).map((entry) => entry.amount),
    );
    final moneyOut = ExactAmount.sum(
      _entries.where((entry) => !entry.moneyIn).map((entry) => entry.amount),
    );
    return Scaffold(
      appBar: AppBar(
        title: Text('${_account.code} · ${_account.name}'),
        actions: [
          IconButton(
            tooltip: 'Reconcile with the statement',
            onPressed: _reconcile,
            icon: const Icon(Icons.fact_check_outlined),
          ),
        ],
      ),
      body: _body(moneyIn, moneyOut),
      bottomNavigationBar: Padding(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
        child: Row(
          children: [
            Expanded(
              child: FilledButton.icon(
                onPressed: () => _record(moneyIn: true),
                icon: const Icon(Icons.south_west),
                label: const Text('Money in'),
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: FilledButton.tonalIcon(
                onPressed: () => _record(moneyIn: false),
                icon: const Icon(Icons.north_east),
                label: const Text('Money out'),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _body(ExactAmount moneyIn, ExactAmount moneyOut) {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'What has moved through ${_account.name}',
        error: _error,
        onRetry: _load,
      );
    }
    return ListView(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                '${_account.balance.toDecimalString()} in this account',
                style: Theme.of(context).textTheme.titleLarge,
              ),
              const SizedBox(height: 4),
              Text(
                'Opening ${_account.openingBalance.toDecimalString()} · '
                '${moneyIn.toDecimalString()} in · ${moneyOut.toDecimalString()} out',
                style: Theme.of(context).textTheme.bodySmall,
              ),
              if (_entries.isNotEmpty) ...[
                const SizedBox(height: 4),
                Text(
                  'Tap an entry to put it in the ledger. The cash book moves the balance; '
                  'the ledger is where the double entry lives.',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ],
          ),
        ),
        const Divider(height: 1),
        if (_entries.isEmpty)
          const Padding(
            padding: EdgeInsets.all(24),
            child: Text(
              'Nothing has moved through this account yet.',
              textAlign: TextAlign.center,
            ),
          ),
        for (final entry in _entries)
          ListTile(
            leading: Icon(
              entry.moneyIn ? Icons.south_west : Icons.north_east,
              color: entry.moneyIn
                  ? Colors.green.shade700
                  : Colors.red.shade700,
            ),
            title: Text(
              entry.description.isEmpty
                  ? (entry.moneyIn ? 'Money in' : 'Money out')
                  : entry.description,
            ),
            subtitle: Text(
              [
                entry.date,
                if (entry.voucher.isNotEmpty) entry.voucher,
                // Only an unusual state is worth saying: a posted entry is the norm, and a line reading "posted" on every
                // row is noise that hides the one that says voided.
                if (entry.status.isNotEmpty && entry.status != 'posted')
                  entry.status,
                if (_posted.contains(entry.id)) 'in the ledger',
              ].join(' · '),
            ),
            trailing: Text(
              '${entry.moneyIn ? '+' : '−'}${entry.amount.toDecimalString()}',
              style: TextStyle(
                fontWeight: FontWeight.bold,
                color: entry.moneyIn
                    ? Colors.green.shade700
                    : Colors.red.shade700,
              ),
            ),
            onTap: () => _post(entry),
          ),
        const Divider(height: 24),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  'Statement checks',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
              ),
              TextButton(onPressed: _reconcile, child: const Text('Check it')),
            ],
          ),
        ),
        if (_checks.isEmpty)
          const Padding(
            padding: EdgeInsets.fromLTRB(16, 0, 16, 24),
            child: Text(
              'Not yet checked against a bank statement. Checking asks what the statement '
              'says, so a month the books and the bank disagree is found in the month it happened.',
            ),
          ),
        for (final check in _checks)
          ListTile(
            leading: Icon(
              check.agrees ? Icons.check_circle_outline : Icons.error_outline,
              color: check.agrees
                  ? Colors.green.shade700
                  : Colors.orange.shade800,
            ),
            title: Text(reconciliationLine(check)),
            subtitle: Text(
              [
                check.status == 'completed' ? 'checked' : 'still to be checked',
                if (check.reviewedBy.isNotEmpty) 'by ${check.reviewedBy}',
              ].join(' · '),
            ),
            trailing: check.status == 'completed'
                ? null
                : TextButton(
                    onPressed: () => _markChecked(check),
                    child: const Text('Checked'),
                  ),
          ),
      ],
    );
  }
}

/// Asks who checked the reconciliation, because the record names them.
class _ReviewerDialog extends StatefulWidget {
  const _ReviewerDialog();

  @override
  State<_ReviewerDialog> createState() => _ReviewerDialogState();
}

class _ReviewerDialogState extends State<_ReviewerDialog> {
  final _who = TextEditingController();

  @override
  void dispose() {
    _who.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Who checked it?'),
      content: TextField(
        controller: _who,
        autofocus: true,
        decoration: const InputDecoration(labelText: 'Name'),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        ElevatedButton(
          onPressed: () {
            if (_who.text.trim().isEmpty) return;
            Navigator.of(context).pop(_who.text.trim());
          },
          child: const Text('Record it'),
        ),
      ],
    );
  }
}

/// The form for one receipt or payment.
class _EntrySheet extends StatefulWidget {
  const _EntrySheet({
    required this.service,
    required this.account,
    required this.moneyIn,
  });

  final CashBookService service;
  final CashAccount account;
  final bool moneyIn;

  @override
  State<_EntrySheet> createState() => _EntrySheetState();
}

class _EntrySheetState extends State<_EntrySheet> {
  final _amount = TextEditingController();
  final _description = TextEditingController();
  final _voucher = TextEditingController();
  final _against = TextEditingController();
  String? _complaint;
  bool _saving = false;

  @override
  void dispose() {
    _amount.dispose();
    _description.dispose();
    _voucher.dispose();
    _against.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final amount = ExactAmount.tryParse(_amount.text.trim());
    if (amount == null) {
      setState(() => _complaint = 'That is not a figure.');
      return;
    }
    if (amount.isZero) {
      setState(() => _complaint = 'An entry has to be more than nothing.');
      return;
    }
    if (widget.moneyIn == false && amount > widget.account.balance) {
      setState(
        () => _complaint =
            'That is more than the ${widget.account.balance.toDecimalString()} in this account. '
            'A payment from money that is not there would leave the book saying the account owes itself.',
      );
      return;
    }
    if (_description.text.trim().isEmpty) {
      setState(() => _complaint = 'Say what the money was for.');
      return;
    }
    setState(() {
      _saving = true;
      _complaint = null;
    });
    try {
      await widget.service.record(
        account: widget.account,
        moneyIn: widget.moneyIn,
        amount: amount,
        description: _description.text,
        date: DateTime.now().toIso8601String().substring(0, 10),
        voucher: _voucher.text.trim(),
        againstAccount: _against.text.trim(),
      );
      if (!mounted) return;
      Navigator.of(context).pop(true);
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
    final incoming = widget.moneyIn;
    return Padding(
      padding: EdgeInsets.only(
        left: 16,
        right: 16,
        top: 16,
        bottom: MediaQuery.of(context).viewInsets.bottom + 16,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            incoming ? 'Money in' : 'Money out',
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: 4),
          Text(
            incoming
                ? 'Into ${widget.account.name}'
                : 'Out of ${widget.account.name}, which holds '
                      '${widget.account.balance.toDecimalString()}',
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _amount,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(labelText: 'Amount'),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _description,
            decoration: const InputDecoration(
              labelText: 'What for',
              helperText: 'Like "Rent for November" or "Paid by Acme"',
            ),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _voucher,
            decoration: const InputDecoration(
              labelText: 'Voucher number, if you have one',
            ),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _against,
            decoration: InputDecoration(
              labelText: incoming
                  ? 'Paid in from account'
                  : 'Paid out to account',
              helperText:
                  'The chart account this belongs to, like 6100 for rent',
            ),
          ),
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
                onPressed: _saving ? null : _save,
                child: Text(_saving ? 'Recording…' : 'Record it'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
