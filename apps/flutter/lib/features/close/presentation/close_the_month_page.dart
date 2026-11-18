// Close the month: making last month's figures final.
//
// The decision in front of a person here is small and consequential: after this, nothing can be posted into that month
// again, and a correction goes into the month they are in now. So the page leads with which month is being offered and
// why, and it will not offer the close while the books have something plainly wrong in them - a draft entry is money
// missing from every statement, and closing a month with a draft in it buries the omission inside a month nobody may
// write to again.
//
// A check that could not be made is never counted as clean: the close is withheld and the page says which check did not
// run, because "probably fine" is not a state a set of books can be closed in.
import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/close/data/close_report.dart';
import 'package:vimbai_mobile_client/features/close/data/close_source.dart';
import 'package:vimbai_mobile_client/features/close/data/month_close.dart';
import 'package:vimbai_mobile_client/features/close/presentation/books_in_order_page.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';

class CloseTheMonthPage extends StatefulWidget {
  const CloseTheMonthPage({
    super.key,
    this.service,
    this.source,
    this.today,
    this.closedBy,
  });

  final ClosedPeriodsService? service;

  /// The three checks, so a test can answer them without a server.
  final BookCloseSource? source;

  /// The day to answer as at. Fixed in a test so the page cannot drift with the clock.
  final String? today;

  /// Who is closing, which the engine records against the month.
  final String? closedBy;

  @override
  State<CloseTheMonthPage> createState() => _CloseTheMonthPageState();
}

class _CloseTheMonthPageState extends State<CloseTheMonthPage> {
  late final ClosedPeriodsService _service =
      widget.service ?? ClosedPeriodsService.instance;
  late final BookCloseSource _source = widget.source ?? RealBookCloseSource();

  MonthClose? _months;
  CloseReport? _checks;
  Object? _failure;
  bool _reading = true;
  bool _closing = false;
  String? _closedNow;

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
    try {
      final months = await _service.read(_today);
      final checks = await _readChecks();
      if (!mounted) return;
      setState(() {
        _months = months;
        _checks = checks;
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

  /// The three checks that already own the question of whether the books are in order.
  Future<CloseReport> _readChecks() async {
    final answers = await Future.wait([
      _trialBalance(),
      _journal(),
      _suppliers(),
    ]);
    final balance = answers[0] as ({TrialBalance? value, String? reason});
    final journal = answers[1] as ({List<JournalEntry>? value, String? reason});
    final suppliers = answers[2] as ({PayablesCheck? value, String? reason});
    return CloseReport([
      CloseItem.drafts(journal.value, reason: journal.reason),
      CloseItem.suppliersControl(suppliers.value, reason: suppliers.reason),
      CloseItem.debitsAndCredits(balance.value, reason: balance.reason),
    ]);
  }

  Future<({TrialBalance? value, String? reason})> _trialBalance() async {
    try {
      return (value: await _source.trialBalance(), reason: null);
    } catch (error) {
      return (value: null, reason: CouldNotRead.reasonOf(error));
    }
  }

  Future<({List<JournalEntry>? value, String? reason})> _journal() async {
    try {
      return (value: await _source.journal(), reason: null);
    } catch (error) {
      return (value: null, reason: CouldNotRead.reasonOf(error));
    }
  }

  Future<({PayablesCheck? value, String? reason})> _suppliers() async {
    try {
      return (value: await _source.suppliersControl(), reason: null);
    } catch (error) {
      return (value: null, reason: CouldNotRead.reasonOf(error));
    }
  }

  Future<void> _close(String month) async {
    final months = _months;
    final checks = _checks;
    if (months == null || checks == null) return;
    final spoken = spokenMonth(month);

    // what closing means, said once, before anything is written
    final agreed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Close $spoken?'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'After this, nothing can be posted into $spoken. A correction goes into '
              'the month you are in now, which is where an accountant puts it anyway.',
            ),
            const SizedBox(height: 12),
            Text(
              checks.isInOrder
                  ? 'The checks are clean: the ledger balances, nothing is in draft, '
                        'and the suppliers agree with their control account.'
                  : 'The checks are not clean yet, so the engine may refuse this.',
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Not yet'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text('Close $spoken'),
          ),
        ],
      ),
    );
    if (agreed != true) return;

    setState(() => _closing = true);
    try {
      await _service.close(
        period: month,
        closesOn: _today,
        closedBy: widget.closedBy ?? '',
      );
      if (!mounted) return;
      setState(() {
        _closing = false;
        _closedNow = month;
      });
      await _load();
    } catch (error) {
      if (!mounted) return;
      setState(() => _closing = false);
      // the engine's own sentence names the month and the way out, so it is shown as it came
      await showDialog<void>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('That month was not closed'),
          content: Text(CouldNotRead.reasonOf(error)),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('Close'),
            ),
          ],
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Close the month'),
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
        what: 'The months closed',
        error: failure,
        onRetry: _load,
      );
    }
    if (_reading) {
      return const Center(child: CircularProgressIndicator());
    }
    final months = _months;
    final checks = _checks;
    if (months == null || checks == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Text(months.headline, style: Theme.of(context).textTheme.titleLarge),
        const SizedBox(height: 6),
        Text(months.detail),
        const SizedBox(height: 16),
        ..._offer(months, checks),
        const SizedBox(height: 24),
        Text('Months closed', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        if (months.closed.isEmpty)
          const Text('No month has been closed yet, so the books are open.'),
        for (final closed in months.closed.reversed) _closedTile(closed),
        const SizedBox(height: 24),
        const Text(
          'A month is closed once its figures have been read, reconciled or filed. '
          'The entries already in it are never changed by closing it.',
        ),
      ],
    );
  }

  /// The month being offered, with the checks that stand in the way of closing it.
  List<Widget> _offer(MonthClose months, CloseReport checks) {
    final month = months.monthOnOffer;
    if (month.isEmpty || months.isClosed(month)) {
      return [
        Card(
          child: ListTile(
            leading: const Icon(Icons.lock_outline),
            title: Text('Nothing to close yet'),
            subtitle: Text(
              '${spokenMonth(months.currentMonth)} is the month you are in, and a '
              'month is closed once it is over.',
            ),
          ),
        ),
      ];
    }

    final spoken = spokenMonth(month);
    final blocked = !checks.isInOrder;
    final firstToFix = checks.firstNeedingYou;

    return [
      Card(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                _closedNow == month
                    ? '$spoken is closed'
                    : 'Ready to close: $spoken',
                style: Theme.of(context).textTheme.titleMedium,
              ),
              const SizedBox(height: 6),
              Text(
                blocked
                    ? 'Fix what the checks found first, or closing it will bury the problem in a '
                          'month nothing may be posted into.'
                    : 'The checks are clean. Closing makes its figures final.',
              ),
              if (firstToFix != null) ...[
                const SizedBox(height: 10),
                Text(
                  '${firstToFix.subject}: ${firstToFix.headline}',
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                const SizedBox(height: 4),
                Text(firstToFix.detail),
                const SizedBox(height: 8),
                OutlinedButton(
                  onPressed: () async {
                    await Navigator.of(context).push(
                      MaterialPageRoute<void>(
                        builder: (_) => const BooksInOrderPage(),
                      ),
                    );
                    await _load();
                  },
                  child: const Text('Open Books in order'),
                ),
              ],
              if (checks.couldNotCheck > 0 && firstToFix == null) ...[
                const SizedBox(height: 8),
                Text(
                  '${checks.couldNotCheck} of the checks could not be made just now, so '
                  'the books are not known to be in order from here.',
                ),
              ],
              const SizedBox(height: 12),
              FilledButton(
                onPressed: _closing || blocked ? null : () => _close(month),
                child: Text(_closing ? 'Closing $spoken' : 'Close $spoken'),
              ),
            ],
          ),
        ),
      ),
    ];
  }

  Widget _closedTile(ClosedPeriod closed) {
    return Card(
      child: ListTile(
        leading: const Icon(Icons.lock),
        title: Text(closed.spoken),
        subtitle: closed.trail.isEmpty ? null : Text(closed.trail),
      ),
    );
  }
}
