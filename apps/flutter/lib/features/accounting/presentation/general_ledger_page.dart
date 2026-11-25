// The general ledger: the Book itself, for one window.
//
// The Ledger screen answers one account at a time, and the trial balance answers one closing figure per account. Neither
// answers the question an accountant asks when a set of books is being closed: what did every account open on, what moved
// it, and what did it close on. This is that report. Every figure comes from the engine, which reads the posted entries;
// nothing is added up on the phone, so the screen cannot disagree with the books it is showing.
import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/data/general_ledger.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/ledger_page.dart';

/// The window the report runs over. A report of a whole Book is read for a period, so the periods people actually ask for
/// are offered by name rather than left to two date pickers.
enum _Window { everything, thisMonth, thisYear }

class GeneralLedgerPage extends StatefulWidget {
  const GeneralLedgerPage({super.key, this.service, this.today});

  /// The engine, injected so a test can answer without a phone.
  final AccountingApiService? service;

  /// The day the periods are counted from, injected for the same reason.
  final DateTime? today;

  @override
  State<GeneralLedgerPage> createState() => _GeneralLedgerPageState();
}

class _GeneralLedgerPageState extends State<GeneralLedgerPage> {
  late final AccountingApiService _apiService =
      widget.service ?? AccountingApiService();
  late DateTime _today = widget.today ?? DateTime.now();
  _Window _window = _Window.everything;
  late Future<GeneralLedgerReport> _report;

  @override
  void initState() {
    super.initState();
    _report = _read();
  }

  /// The window as the engine wants it: a start and an end, or neither.
  (DateTime?, DateTime?) get _dates {
    switch (_window) {
      case _Window.everything:
        return (null, null);
      case _Window.thisMonth:
        final first = DateTime(_today.year, _today.month);
        // the last day of the month is the day before the first of the next, which is how a month ends everywhere
        final last = DateTime(_today.year, _today.month + 1, 0);
        return (first, last);
      case _Window.thisYear:
        return (DateTime(_today.year), DateTime(_today.year, 12, 31));
    }
  }

  Future<GeneralLedgerReport> _read() {
    final (start, end) = _dates;
    return _apiService.getGeneralLedger(startDate: start, endDate: end);
  }

  void _choose(_Window window) {
    setState(() {
      _window = window;
      _report = _read();
    });
  }

  void _openAccount(GeneralLedgerAccount account) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) =>
            LedgerPage(service: widget.service, account: account.accountNumber),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('General ledger')),
      body: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 0),
            child: Wrap(
              spacing: 8,
              children: [
                for (final window in _Window.values)
                  ChoiceChip(
                    label: Text(switch (window) {
                      _Window.everything => 'Everything',
                      _Window.thisMonth => 'This month',
                      _Window.thisYear => 'This year',
                    }),
                    selected: _window == window,
                    onSelected: (_) => _choose(window),
                  ),
              ],
            ),
          ),
          Expanded(
            child: FutureBuilder<GeneralLedgerReport>(
              future: _report,
              builder: (context, snapshot) {
                if (snapshot.connectionState == ConnectionState.waiting) {
                  return const Center(child: CircularProgressIndicator());
                }
                if (snapshot.hasError) {
                  return CouldNotRead(
                    what: 'The general ledger',
                    error: snapshot.error,
                    onRetry: () => setState(() => _report = _read()),
                  );
                }
                final report = snapshot.data;
                if (report == null) {
                  return const Center(
                    child: Text('No data for General ledger.'),
                  );
                }
                return _Report(report: report, onOpenAccount: _openAccount);
              },
            ),
          ),
        ],
      ),
    );
  }
}

class _Report extends StatelessWidget {
  const _Report({required this.report, required this.onOpenAccount});

  final GeneralLedgerReport report;
  final void Function(GeneralLedgerAccount) onOpenAccount;

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 24),
      children: [
        Text(
          report.windowLabel,
          style: Theme.of(
            context,
          ).textTheme.titleMedium?.copyWith(fontWeight: FontWeight.bold),
        ),
        const SizedBox(height: 4),
        Text(report.summaryLabel, style: Theme.of(context).textTheme.bodySmall),
        const SizedBox(height: 8),
        // the proof the report has to carry: what the window debited, what it credited, and whether they agree
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _Figure(label: 'Debits', amount: report.totalDebit),
                _Figure(label: 'Credits', amount: report.totalCredit),
                const SizedBox(height: 4),
                Text(
                  report.isBalanced
                      ? 'The window debits what it credits.'
                      : 'The window does not balance: the debits and the credits differ.',
                  style: TextStyle(
                    fontWeight: FontWeight.w600,
                    color: report.isBalanced
                        ? Theme.of(context).colorScheme.primary
                        : Theme.of(context).colorScheme.error,
                  ),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 8),
        for (final account in report.accounts)
          _AccountCard(account: account, onOpen: () => onOpenAccount(account)),
      ],
    );
  }
}

class _Figure extends StatelessWidget {
  const _Figure({required this.label, required this.amount});

  final String label;
  final ExactAmount amount;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [Text(label), Text(amount.toDecimalString())],
    );
  }
}

class _AccountCard extends StatelessWidget {
  const _AccountCard({required this.account, required this.onOpen});

  final GeneralLedgerAccount account;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    return Card(
      clipBehavior: Clip.antiAlias,
      child: ExpansionTile(
        title: Text('${account.accountNumber} ${account.accountName}'),
        subtitle: Text(
          account.moved
              ? 'Opened ${account.startBalance.toDecimalString()} · closed ${account.endBalance.toDecimalString()}'
              : 'Did not move in this window',
        ),
        trailing: IconButton(
          icon: const Icon(Icons.open_in_new),
          tooltip: 'Open this account',
          onPressed: onOpen,
        ),
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _Figure(label: 'Opening balance', amount: account.startBalance),
                _Figure(label: 'Debits', amount: account.totalDebit),
                _Figure(label: 'Credits', amount: account.totalCredit),
                _Figure(label: 'Closing balance', amount: account.endBalance),
                if (account.entries.isNotEmpty) const Divider(),
                for (final row in account.entries) _Row(row: row),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _Row extends StatelessWidget {
  const _Row({required this.row});

  final GeneralLedgerRow row;

  @override
  Widget build(BuildContext context) {
    final amount = row.debit != ExactAmount.zero
        ? row.debit.toDecimalString()
        : row.credit.toDecimalString();
    final side = row.debit != ExactAmount.zero ? 'Dr' : 'Cr';
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Expanded(
                child: Text(
                  row.description.isEmpty ? row.sourceModule : row.description,
                ),
              ),
              Text('$side $amount'),
            ],
          ),
          Text(
            '${row.entryDate} · balance ${row.balance.toDecimalString()}',
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}
