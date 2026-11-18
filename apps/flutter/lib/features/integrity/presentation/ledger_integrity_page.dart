// Proof nothing has been changed: the answer to "are my books still the books I wrote?"
//
// The value of this screen is that it can come back bad. A page that always says "all good" teaches a person to stop reading it,
// so an empty book says it has nothing to prove, a broken chain names the entry that does not match, and a failed read says the
// check did not run rather than showing a tick nobody earned.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/integrity/data/ledger_integrity.dart';

abstract class LedgerIntegritySource {
  Future<LedgerIntegrity> check();
}

class RealLedgerIntegritySource implements LedgerIntegritySource {
  RealLedgerIntegritySource({AccountingApiService? accounting})
    : _accounting = accounting ?? AccountingApiService();

  final AccountingApiService _accounting;

  @override
  Future<LedgerIntegrity> check() => _accounting.getLedgerIntegrity();
}

class LedgerIntegrityPage extends StatefulWidget {
  const LedgerIntegrityPage({super.key, this.source});

  final LedgerIntegritySource? source;

  @override
  State<LedgerIntegrityPage> createState() => _LedgerIntegrityPageState();
}

class _LedgerIntegrityPageState extends State<LedgerIntegrityPage> {
  late final LedgerIntegritySource _source =
      widget.source ?? RealLedgerIntegritySource();

  bool _loading = true;
  LedgerIntegrity? _integrity;
  Object? _error;

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
      final integrity = await _source.check();
      if (!mounted) return;
      setState(() {
        _integrity = integrity;
        _loading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _error = error;
        _integrity = null;
        _loading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Proof nothing has changed'),
        actions: [
          IconButton(
            tooltip: 'Check again',
            onPressed: _loading ? null : _load,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: _body(),
    );
  }

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'The proof of the books',
        error: _error,
        onRetry: _load,
      );
    }
    final integrity = _integrity;
    if (integrity == null) return const SizedBox.shrink();

    if (integrity.isProofOfNothing) {
      return _message(
        icon: Icons.inbox_outlined,
        headline: 'There is nothing to prove yet.',
        detail:
            'No entry has been posted in this Book. The check walks posted entries, '
            'so it can say something useful once the first one is in.',
      );
    }

    if (!integrity.valid) {
      return ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
        children: [
          _verdict(
            icon: Icons.report_problem_outlined,
            colour: Theme.of(context).colorScheme.error,
            headline:
                'The books no longer match what was written. '
                '${integrity.problems.length} '
                '${integrity.problems.length == 1 ? 'problem was' : 'problems were'} '
                'found in ${integrity.entriesChecked} '
                '${integrity.entriesChecked == 1 ? 'entry' : 'entries'}.',
            detail:
                'Nothing was lost, and nothing can be quietly put back: a posted entry is '
                'not edited in this system. What was changed is named below, and a correcting '
                'entry is how the books are put right.',
          ),
          const SizedBox(height: 16),
          for (final problem in integrity.problems)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Text(problem.meaning),
            ),
          const SizedBox(height: 8),
          const _Explain(
            'Each entry carries a fingerprint of its own contents and of the entry before '
            'it. Changing any figure in a past entry breaks both, which is how this is '
            'noticed at all.',
          ),
          if (integrity.legacyStamped > 0) ...[
            const SizedBox(height: 12),
            _Explain(
              '${integrity.legacyStamped} of these entries were written before fingerprints '
              'covered the exact amounts; they verify under the older rule.',
            ),
          ],
        ],
      );
    }

    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
      children: [
        _verdict(
          icon: Icons.verified_outlined,
          colour: Theme.of(context).colorScheme.primary,
          headline:
              'All ${integrity.entriesChecked} '
              '${integrity.entriesChecked == 1 ? 'entry links' : 'entries link'} '
              'together. Nothing has changed since it was written.',
          detail:
              'The check read every posted entry in this Book and found each one matching '
              'its fingerprint and the entry before it.',
        ),
        const SizedBox(height: 20),
        _hashRow(integrity),
        const SizedBox(height: 16),
        if (integrity.legacyStamped > 0)
          _Explain(
            '${integrity.legacyStamped} of these entries were written before fingerprints '
            'covered the exact amounts; they verify under the older rule and are not '
            'themselves a problem.',
          ),
        const _Explain(
          'This proves nothing in the books has changed since it was saved. It does not '
          'prove a figure was right when it was entered; that is what the reports and the '
          'checks in Books in order are for.',
        ),
      ],
    );
  }

  /// The head of the chain: the fingerprint the next entry will be built on.
  Widget _hashRow(LedgerIntegrity integrity) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('The newest fingerprint', style: theme.textTheme.labelLarge),
        const SizedBox(height: 4),
        Row(
          children: [
            Expanded(
              child: Text(
                integrity.shortHeadHash,
                style: theme.textTheme.bodyMedium?.copyWith(
                  fontFamily: 'monospace',
                ),
              ),
            ),
            TextButton.icon(
              onPressed: () async {
                await Clipboard.setData(
                  ClipboardData(text: integrity.headHash),
                );
              },
              icon: const Icon(Icons.copy, size: 18),
              label: const Text('Copy it all'),
            ),
          ],
        ),
      ],
    );
  }

  Widget _verdict({
    required IconData icon,
    required Color colour,
    required String headline,
    required String detail,
  }) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Icon(icon, size: 40, color: colour),
        const SizedBox(height: 8),
        Text(
          headline,
          style: theme.textTheme.titleMedium?.copyWith(
            fontWeight: FontWeight.bold,
          ),
        ),
        const SizedBox(height: 8),
        Text(detail),
      ],
    );
  }

  Widget _message({
    required IconData icon,
    required String headline,
    required String detail,
  }) => Center(
    child: Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(icon, size: 48),
          const SizedBox(height: 12),
          Text(
            headline,
            style: const TextStyle(fontWeight: FontWeight.bold),
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 6),
          Text(detail, textAlign: TextAlign.center),
        ],
      ),
    ),
  );
}

class _Explain extends StatelessWidget {
  const _Explain(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 8),
    child: Text(text, style: Theme.of(context).textTheme.bodySmall),
  );
}
