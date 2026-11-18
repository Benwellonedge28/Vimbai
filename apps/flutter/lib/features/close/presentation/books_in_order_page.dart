// Books in order: the three questions a person asks before they trust a set of accounts.
//
// The Book has statements on every screen, but nothing said whether the ledger underneath them adds up, whether entries were
// left in draft where no statement can see them, or whether the suppliers still agree with the account that stands for them.
// This page answers those three in the engine's own figures, says which one to fix first, and opens the screen that fixes it.
//
// A check that could not be made is shown as such: three green ticks over a check that never ran would be worse than no page.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entries_list_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/trial_balance_page.dart';
import 'package:vimbai_mobile_client/features/close/data/close_report.dart';
import 'package:vimbai_mobile_client/features/close/data/close_source.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/payables_page.dart';

class BooksInOrderPage extends StatefulWidget {
  const BooksInOrderPage({super.key, this.source});

  /// A test answers the three readings itself, so the page can be exercised without a server.
  final BookCloseSource? source;

  @override
  State<BooksInOrderPage> createState() => _BooksInOrderPageState();
}

class _BooksInOrderPageState extends State<BooksInOrderPage> {
  late final BookCloseSource _source = widget.source ?? RealBookCloseSource();

  bool _loading = true;
  CloseReport? _report;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
    });
    // the three at once: each is its own door and none needs another's answer
    final answers = await Future.wait([
      _trialBalance(),
      _journal(),
      _suppliers(),
    ]);
    final balance = answers[0] as ({TrialBalance? value, String? reason});
    final journal = answers[1] as ({List<JournalEntry>? value, String? reason});
    final suppliers = answers[2] as ({PayablesCheck? value, String? reason});

    final report = CloseReport([
      CloseItem.drafts(journal.value, reason: journal.reason),
      CloseItem.suppliersControl(suppliers.value, reason: suppliers.reason),
      CloseItem.debitsAndCredits(balance.value, reason: balance.reason),
    ]);
    if (!mounted) return;
    setState(() {
      _report = report;
      _loading = false;
    });
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

  /// The screen that fixes each check, in the same order the report is built.
  Widget _doorFor(String subject) {
    switch (subject) {
      case 'Entries still in draft':
        return const JournalEntriesListPage();
      case 'Suppliers and their control account':
        return const PayablesPage();
      default:
        return const TrialBalancePage();
    }
  }

  @override
  Widget build(BuildContext context) {
    final report = _report;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Books in order'),
        actions: [
          IconButton(
            tooltip: 'Check again',
            onPressed: _loading ? null : _load,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : RefreshIndicator(
              onRefresh: _load,
              child: ListView(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
                children: [
                  if (report != null) ...[
                    _summary(report),
                    const SizedBox(height: 12),
                    for (final item in report.items) _check(item),
                  ],
                ],
              ),
            ),
    );
  }

  /// The one line that answers the question, before the three reasons underneath it.
  Widget _summary(CloseReport report) {
    final String sentence;
    if (report.isInOrder) {
      sentence =
          'This Book is in order: all ${report.items.length} checks came back '
          'clean.';
    } else if (report.needsYou > 0) {
      final first = report.firstNeedingYou;
      sentence =
          '${report.needsYou} ${report.needsYou == 1 ? 'thing needs' : 'things need'} '
          'your attention'
          '${first == null ? '' : ', starting with ${first.subject.toLowerCase()}'}'
          '.';
    } else {
      sentence =
          'Nothing is known to be wrong, but '
          '${report.couldNotCheck} ${report.couldNotCheck == 1 ? 'check' : 'checks'} '
          'could not be made.';
    }
    return Text(sentence, style: Theme.of(context).textTheme.titleMedium);
  }

  Widget _check(CloseItem item) {
    final (icon, colour) = switch (item.verdict) {
      CloseVerdict.inOrder => (
        Icons.check_circle_outline,
        Theme.of(context).colorScheme.primary,
      ),
      CloseVerdict.needsYou => (
        Icons.error_outline,
        Theme.of(context).colorScheme.error,
      ),
      CloseVerdict.couldNotCheck => (
        Icons.cloud_off,
        Theme.of(context).colorScheme.outline,
      ),
    };
    return Card(
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: () => Navigator.of(
          context,
        ).push(MaterialPageRoute<void>(builder: (_) => _doorFor(item.subject))),
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(icon, color: colour, size: 28),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      item.subject,
                      style: Theme.of(context).textTheme.labelLarge,
                    ),
                    const SizedBox(height: 4),
                    Text(
                      item.headline,
                      style: Theme.of(context).textTheme.bodyLarge?.copyWith(
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    const SizedBox(height: 6),
                    Text(item.detail),
                  ],
                ),
              ),
              const Icon(Icons.chevron_right),
            ],
          ),
        ),
      ),
    );
  }
}
