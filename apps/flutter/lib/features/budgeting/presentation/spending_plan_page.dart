// What this Book planned to take up, and what it actually took up.
//
// The plan is kept with the Book, so everyone the Book is shared with reads the same one. The page leads with the answer
// rather than the plan: is this Book inside what it allowed itself, and if not, which account is past its line. The lines
// underneath are the working, not the point.
//
// A plan that could not be read says so in its own place. A plan is not "zero spent" because the engine did not answer,
// and showing an empty report in place of a failed one would tell a person their spending was fine when nobody checked.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/budgeting/data/spending_plan.dart';
import 'package:vimbai_mobile_client/features/budgeting/data/spending_plan_service.dart';

/// One plan, and what the ledger says about it.
class PlannedAgainst {
  PlannedAgainst({required this.plan, this.variance, this.error});

  final SpendingPlan plan;

  /// The plan against the ledger, or null when that could not be read.
  final PlanVariance? variance;

  /// Why the comparison could not be read, when it could not.
  final String? error;

  bool get readable => variance != null;
}

class SpendingPlanPage extends StatefulWidget {
  const SpendingPlanPage({super.key, this.service, this.accounts});

  /// A test passes its own service so the page can be exercised without a server.
  final SpendingPlanService? service;

  /// A test passes the chart, so the form can be exercised without a server.
  final AccountingApiService? accounts;

  @override
  State<SpendingPlanPage> createState() => _SpendingPlanPageState();
}

class _SpendingPlanPageState extends State<SpendingPlanPage> {
  late final SpendingPlanService _plans =
      widget.service ?? SpendingPlanService.instance;
  late final AccountingApiService _accounts =
      widget.accounts ?? AccountingApiService();

  bool _loading = true;
  String? _error;
  List<PlannedAgainst> _rows = const [];

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
      final plans = await _plans.plans();
      final rows = <PlannedAgainst>[];
      for (final plan in plans) {
        try {
          rows.add(
            PlannedAgainst(
              plan: plan,
              variance: await _plans.variance(plan.id),
            ),
          );
        } catch (failure) {
          rows.add(
            PlannedAgainst(plan: plan, error: CouldNotRead.reasonOf(failure)),
          );
        }
      }
      if (!mounted) return;
      setState(() {
        _rows = rows;
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

  Future<void> _keepAPlan() async {
    final kept = await KeepPlanSheet.show(
      context,
      service: _plans,
      accounts: _accounts,
    );
    if (kept == true) await _load();
  }

  Future<void> _open(PlannedAgainst row) async {
    final changed = await Navigator.of(context).push<bool>(
      MaterialPageRoute(
        builder: (_) => PlanDetailPage(
          plan: row.plan,
          variance: row.variance,
          readError: row.error,
          service: _plans,
        ),
      ),
    );
    if (changed == true) await _load();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('What you planned')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _keepAPlan,
        icon: const Icon(Icons.add),
        label: const Text('Keep a plan'),
      ),
      body: _body(context),
    );
  }

  Widget _body(BuildContext context) {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'what this Book planned',
        error: _error,
        onRetry: _load,
      );
    }
    if (_rows.isEmpty) return _nothingYet(context);
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 96),
        children: [for (final row in _rows) _planCard(context, row)],
      ),
    );
  }

  Widget _nothingYet(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.savings_outlined,
              size: 48,
              color: Theme.of(context).colorScheme.primary,
            ),
            const SizedBox(height: 16),
            Text(
              'Nothing planned yet',
              style: Theme.of(context).textTheme.titleMedium,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 8),
            Text(
              'A plan says what each account is meant to take up over a period. '
              'Once it is kept with the Book, everyone the Book is shared with '
              'reads the same one, and this screen says whether the spending '
              'stayed inside it.',
              textAlign: TextAlign.center,
              style: Theme.of(context).textTheme.bodyMedium,
            ),
          ],
        ),
      ),
    );
  }

  Widget _planCard(BuildContext context, PlannedAgainst row) {
    final variance = row.variance;
    final over = variance?.isOver ?? false;
    return Card(
      margin: const EdgeInsets.only(bottom: 12),
      child: InkWell(
        onTap: () => _open(row),
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      row.plan.name,
                      style: Theme.of(context).textTheme.titleMedium,
                    ),
                  ),
                  if (variance != null)
                    Chip(
                      label: Text(over ? 'Over plan' : 'Within plan'),
                      backgroundColor: over
                          ? Theme.of(context).colorScheme.errorContainer
                          : null,
                    ),
                ],
              ),
              const SizedBox(height: 4),
              Text(
                '${row.plan.startDate} to ${row.plan.endDate}',
                style: Theme.of(context).textTheme.bodySmall,
              ),
              const SizedBox(height: 10),
              if (variance != null) ...[
                Text(
                  variance.headline,
                  style: Theme.of(context).textTheme.bodyLarge,
                ),
                const SizedBox(height: 6),
                Text(
                  '${variance.totalAgainstPlan.toDecimalString()} against '
                  '${variance.totalBudgeted.toDecimalString()} planned '
                  'across ${row.plan.lineCount} '
                  '${row.plan.lineCount == 1 ? 'account' : 'accounts'}',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ] else
                Text(
                  'The comparison could not be read: ${row.error}',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// One plan against the ledger, account by account.
class PlanDetailPage extends StatefulWidget {
  const PlanDetailPage({
    super.key,
    required this.plan,
    this.variance,
    this.readError,
    this.service,
  });

  final SpendingPlan plan;
  final PlanVariance? variance;
  final String? readError;
  final SpendingPlanService? service;

  @override
  State<PlanDetailPage> createState() => _PlanDetailPageState();
}

class _PlanDetailPageState extends State<PlanDetailPage> {
  late final SpendingPlanService _plans =
      widget.service ?? SpendingPlanService.instance;

  late PlanVariance? _variance = widget.variance;
  late String? _error = widget.readError;

  Future<void> _read() async {
    setState(() => _error = null);
    try {
      final variance = await _plans.variance(widget.plan.id);
      if (!mounted) return;
      setState(() => _variance = variance);
    } catch (failure) {
      if (!mounted) return;
      setState(() => _error = CouldNotRead.reasonOf(failure));
    }
  }

  Future<void> _remove() async {
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Remove this plan?'),
        content: const Text(
          'The plan and its lines go. The entries the ledger already holds '
          'are not touched: this removes what was planned, not what was spent.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Keep it'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Remove it'),
          ),
        ],
      ),
    );
    if (sure != true) return;
    try {
      await _plans.remove(widget.plan.id);
      if (!mounted) return;
      Navigator.of(context).pop(true);
    } catch (failure) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            'Could not remove the plan: '
            '${CouldNotRead.reasonOf(failure)}',
          ),
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final variance = _variance;
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.plan.name),
        actions: [
          IconButton(
            onPressed: _remove,
            icon: const Icon(Icons.delete_outline),
            tooltip: 'Remove this plan',
          ),
        ],
      ),
      body: RefreshIndicator(
        onRefresh: _read,
        child: ListView(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
          children: [
            Text(
              '${widget.plan.startDate} to ${widget.plan.endDate}',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 12),
            if (variance != null) ...[
              _verdict(context, variance),
              const SizedBox(height: 20),
              Text(
                'Account by account',
                style: Theme.of(context).textTheme.titleMedium,
              ),
              const SizedBox(height: 8),
              for (final line in variance.lines) _lineRow(context, line),
            ] else if (_error != null)
              CouldNotRead(
                what: 'this plan against the ledger',
                error: _error,
                onRetry: _read,
              ),
          ],
        ),
      ),
    );
  }

  Widget _verdict(BuildContext context, PlanVariance variance) {
    final over = variance.isOver;
    return Card(
      color: over
          ? Theme.of(context).colorScheme.errorContainer
          : Theme.of(context).colorScheme.surfaceContainerHighest,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              variance.headline,
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 8),
            Text(
              '${variance.totalAgainstPlan.toDecimalString()} against plan, '
              'of ${variance.totalBudgeted.toDecimalString()} allowed',
              style: Theme.of(context).textTheme.bodyMedium,
            ),
            if (variance.accountsOver > 0) ...[
              const SizedBox(height: 6),
              Text(
                variance.accountsOver == 1
                    ? 'One account has gone past its line.'
                    : '${variance.accountsOver} accounts have gone past their '
                          'lines.',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _lineRow(BuildContext context, VarianceLine line) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  line.category.trim().isEmpty
                      ? line.accountNumber
                      : line.category,
                  style: Theme.of(context).textTheme.bodyLarge,
                ),
                const SizedBox(height: 2),
                Text(
                  '${line.againstPlan.toDecimalString()} of '
                  '${line.budgeted.toDecimalString()} planned',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ),
          ),
          const SizedBox(width: 12),
          Text(
            line.isOver
                ? '${line.pastBy.toDecimalString()} over'
                : '${line.remaining.toDecimalString()} left',
            style: Theme.of(context).textTheme.bodyMedium?.copyWith(
              color: line.isOver ? Theme.of(context).colorScheme.error : null,
            ),
          ),
        ],
      ),
    );
  }
}

/// The form that keeps a plan with the Book.
class KeepPlanSheet extends StatefulWidget {
  const KeepPlanSheet({
    super.key,
    required this.service,
    required this.accounts,
  });

  final SpendingPlanService service;
  final AccountingApiService accounts;

  /// Opens the form and answers true when a plan was kept.
  static Future<bool?> show(
    BuildContext context, {
    required SpendingPlanService service,
    required AccountingApiService accounts,
  }) {
    return showModalBottomSheet<bool>(
      context: context,
      isScrollControlled: true,
      builder: (_) => Padding(
        padding: EdgeInsets.only(
          bottom: MediaQuery.of(context).viewInsets.bottom,
        ),
        child: KeepPlanSheet(service: service, accounts: accounts),
      ),
    );
  }

  @override
  State<KeepPlanSheet> createState() => _KeepPlanSheetState();
}

class _KeepPlanSheetState extends State<KeepPlanSheet> {
  final _name = TextEditingController();
  final _start = TextEditingController();
  final _end = TextEditingController();

  List<Account> _chart = const [];
  bool _loadingChart = true;
  String? _chartError;

  /// The lines typed so far: an account, what it is for, and what it may take up.
  final List<_DraftLine> _lines = [_DraftLine()];

  bool _saving = false;
  String? _refusal;

  @override
  void initState() {
    super.initState();
    _loadChart();
  }

  @override
  void dispose() {
    _name.dispose();
    _start.dispose();
    _end.dispose();
    for (final line in _lines) {
      line.dispose();
    }
    super.dispose();
  }

  Future<void> _loadChart() async {
    try {
      final chart = await widget.accounts.getAllAccounts();
      if (!mounted) return;
      setState(() {
        _chart = chart;
        _loadingChart = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _chartError = CouldNotRead.reasonOf(failure);
        _loadingChart = false;
      });
    }
  }

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _refusal = null;
    });
    try {
      final lines = <PlanLine>[];
      for (final draft in _lines) {
        final amount = ExactAmount.tryParse(draft.amount.text);
        if (amount == null) {
          throw Exception(
            draft.category.text.trim().isEmpty
                ? 'One of the lines has no figure that reads as an amount.'
                : 'The line for ${draft.category.text.trim()} has no figure '
                      'that reads as an amount.',
          );
        }
        lines.add(
          PlanLine(
            category: draft.category.text.trim(),
            accountNumber: draft.account ?? '',
            budgeted: amount,
          ),
        );
      }
      await widget.service.keep(
        name: _name.text,
        startDate: _start.text,
        endDate: _end.text,
        lines: lines,
      );
      if (!mounted) return;
      Navigator.of(context).pop(true);
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _refusal = CouldNotRead.reasonOf(failure);
        _saving = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 16),
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Keep a plan',
                style: Theme.of(context).textTheme.titleLarge,
              ),
              const SizedBox(height: 4),
              Text(
                'The plan is kept with this Book, so everyone it is shared with '
                'reads the same one.',
                style: Theme.of(context).textTheme.bodySmall,
              ),
              const SizedBox(height: 16),
              TextField(
                controller: _name,
                decoration: const InputDecoration(
                  labelText: 'What to call it',
                  hintText: '2026 plan',
                ),
              ),
              const SizedBox(height: 12),
              Row(
                children: [
                  Expanded(
                    child: TextField(
                      controller: _start,
                      decoration: const InputDecoration(
                        labelText: 'From',
                        hintText: '2026-01-01',
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: TextField(
                      controller: _end,
                      decoration: const InputDecoration(
                        labelText: 'To',
                        hintText: '2026-12-31',
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 20),
              Text(
                'What each account may take up',
                style: Theme.of(context).textTheme.titleMedium,
              ),
              const SizedBox(height: 8),
              if (_loadingChart)
                const Padding(
                  padding: EdgeInsets.symmetric(vertical: 16),
                  child: Center(child: CircularProgressIndicator()),
                )
              else if (_chartError != null)
                CouldNotRead(
                  what: 'the chart of accounts',
                  error: _chartError,
                  onRetry: _loadChart,
                )
              else ...[
                for (var index = 0; index < _lines.length; index++)
                  _lineFields(context, index),
                TextButton.icon(
                  onPressed: () => setState(() => _lines.add(_DraftLine())),
                  icon: const Icon(Icons.add),
                  label: const Text('Another account'),
                ),
              ],
              if (_refusal != null) ...[
                const SizedBox(height: 8),
                Text(
                  _refusal!,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ],
              const SizedBox(height: 16),
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  TextButton(
                    onPressed: _saving
                        ? null
                        : () => Navigator.of(context).pop(false),
                    child: const Text('Not now'),
                  ),
                  const SizedBox(width: 8),
                  FilledButton(
                    onPressed: _saving ? null : _save,
                    child: Text(_saving ? 'Keeping it' : 'Keep the plan'),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _lineFields(BuildContext context, int index) {
    final draft = _lines[index];
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          DropdownButtonFormField<String>(
            initialValue: draft.account,
            isExpanded: true,
            decoration: const InputDecoration(labelText: 'Account'),
            items: [
              for (final account in _chart)
                DropdownMenuItem(
                  value: account.accountNumber,
                  child: Text(
                    '${account.accountNumber} ${account.accountName}',
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
            ],
            onChanged: (value) => setState(() => draft.account = value),
          ),
          const SizedBox(height: 8),
          Row(
            children: [
              Expanded(
                flex: 2,
                child: TextField(
                  controller: draft.category,
                  decoration: const InputDecoration(
                    labelText: 'What it is for',
                    hintText: 'Rent',
                  ),
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                flex: 2,
                child: TextField(
                  controller: draft.amount,
                  keyboardType: const TextInputType.numberWithOptions(
                    decimal: true,
                  ),
                  decoration: const InputDecoration(labelText: 'Amount'),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// A line being typed, before it is a [PlanLine].
class _DraftLine {
  String? account;
  final TextEditingController category = TextEditingController();
  final TextEditingController amount = TextEditingController();

  void dispose() {
    category.dispose();
    amount.dispose();
  }
}
