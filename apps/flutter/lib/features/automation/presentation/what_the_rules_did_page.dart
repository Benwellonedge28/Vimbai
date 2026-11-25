// What the rules did: the outcomes of a Book's automation, and nothing to operate.
//
// The rules run in the background, so this screen has no switch, no schedule and no start button (ADR-013). It answers the
// only question a person has: what did the automation do to my books, and did anything stop?
//
// A run that finished says what it posted in a sentence; a run that stopped says why in the engine's own words, because a
// refusal nobody can read is the same as no answer.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/automation/data/rule_outcomes.dart';

class WhatTheRulesDidPage extends StatefulWidget {
  const WhatTheRulesDidPage({super.key, this.outcomes});

  /// A test passes its own service so the page can be exercised without a server.
  final RuleOutcomesService? outcomes;

  @override
  State<WhatTheRulesDidPage> createState() => _WhatTheRulesDidPageState();
}

class _WhatTheRulesDidPageState extends State<WhatTheRulesDidPage> {
  late final RuleOutcomesService _outcomes =
      widget.outcomes ?? RuleOutcomesService.instance;

  List<RuleRun> _runs = const [];
  bool _reading = true;
  String? _problem;

  @override
  void initState() {
    super.initState();
    _read();
  }

  Future<void> _read() async {
    setState(() {
      _reading = true;
      _problem = null;
    });
    try {
      final runs = await _outcomes.read();
      if (!mounted) return;
      setState(() {
        _runs = runs;
        _reading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _problem = error.toString().replaceFirst('Exception: ', '');
        _reading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('What the rules did'),
        actions: [
          IconButton(
            onPressed: _reading ? null : _read,
            icon: const Icon(Icons.refresh),
            tooltip: 'Read the runs again',
          ),
        ],
      ),
      body: RefreshIndicator(onRefresh: _read, child: _body(context)),
    );
  }

  Widget _body(BuildContext context) {
    final theme = Theme.of(context);
    if (_reading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_problem != null) {
      return ListView(
        padding: const EdgeInsets.all(16),
        children: [
          CouldNotRead(
            what: 'What the rules did',
            error: _problem,
            onRetry: _read,
          ),
        ],
      );
    }
    if (_runs.isEmpty) {
      return ListView(
        padding: const EdgeInsets.all(24),
        children: [
          Text(
            'Nothing has run yet.',
            style: theme.textTheme.titleMedium,
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 8),
          Text(
            'The rules run on their own. What they do to the books will appear here.',
            style: theme.textTheme.bodyMedium,
            textAlign: TextAlign.center,
          ),
        ],
      );
    }
    return ListView.separated(
      padding: const EdgeInsets.all(16),
      itemCount: _runs.length + 1,
      separatorBuilder: (context, index) => const SizedBox(height: 12),
      itemBuilder: (context, index) {
        if (index == 0) {
          return Text(
            'Your rules ran on their own. This is what they did.',
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          );
        }
        return _RunCard(run: _runs[index - 1]);
      },
    );
  }
}

/// One run, as a person reads it.
class _RunCard extends StatelessWidget {
  const _RunCard({required this.run});

  final RuleRun run;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final stopped = run.stopped;
    final icon = run.stillRunning
        ? Icons.hourglass_top
        : (stopped ? Icons.error_outline : Icons.check_circle_outline);
    final colour = run.stillRunning
        ? theme.colorScheme.onSurfaceVariant
        : (stopped ? theme.colorScheme.error : theme.colorScheme.primary);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(icon, size: 20, color: colour),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    run.ruleName.isEmpty ? 'A rule' : run.ruleName,
                    style: theme.textTheme.titleMedium,
                  ),
                ),
                Text(run.dayReads, style: theme.textTheme.bodySmall),
              ],
            ),
            const SizedBox(height: 8),
            Text(run.whatItDid, style: theme.textTheme.bodyMedium),
            if (!stopped && !run.stillRunning && run.steps.length > 1) ...[
              const SizedBox(height: 8),
              for (final step in run.steps)
                Padding(
                  padding: const EdgeInsets.only(bottom: 2),
                  child: Text(
                    '· ${step.whatItDid}',
                    style: theme.textTheme.bodySmall,
                  ),
                ),
            ],
          ],
        ),
      ),
    );
  }
}
