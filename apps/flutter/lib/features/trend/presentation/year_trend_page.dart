// The year, month by month, for a Book that keeps proper accounts.
//
// This is the question a year-to-date profit cannot answer: is the trade steady, growing, or carried by one good month? Each
// month is the engine's own income statement for that month, so nothing here is summed on the phone.
//
// Months that could not be read are shown as unread, and while any is unread the year total is withheld: a total built from
// the months that happened to answer would be a quiet lie.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/income_statement_page.dart';
import 'package:vimbai_mobile_client/features/trend/data/month_trend.dart';

abstract class YearTrendSource {
  /// The month that starts [start] and ends the day before [end].
  Future<IncomeStatement> month(DateTime start, DateTime end);
}

class RealYearTrendSource implements YearTrendSource {
  RealYearTrendSource({AccountingApiService? accounting})
    : _accounting = accounting ?? AccountingApiService();

  final AccountingApiService _accounting;

  @override
  Future<IncomeStatement> month(DateTime start, DateTime end) =>
      _accounting.getIncomeStatement(start, end);
}

class YearTrendPage extends StatefulWidget {
  const YearTrendPage({super.key, this.source, this.today});

  final YearTrendSource? source;

  /// A test pins the year so "so far" does not drift under it.
  final DateTime? today;

  @override
  State<YearTrendPage> createState() => _YearTrendPageState();
}

class _YearTrendPageState extends State<YearTrendPage> {
  late final YearTrendSource _source = widget.source ?? RealYearTrendSource();
  late final DateTime _today = widget.today ?? DateTime.now();

  bool _loading = true;
  YearTrend? _trend;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
    });
    final months = monthsSoFar(_today);
    // every month at once: they are independent doors and the newest month is not more important than the oldest
    final figures = await Future.wait(months.map(_month));
    if (!mounted) return;
    setState(() {
      _trend = YearTrend(figures);
      _loading = false;
    });
  }

  Future<MonthFigure> _month(DateTime start) async {
    try {
      return MonthFigure.of(
        start,
        await _source.month(start, endOfMonth(start)),
      );
    } catch (_) {
      // one month that could not be read must not take the rest of the year with it
      return MonthFigure.unread(start);
    }
  }

  @override
  Widget build(BuildContext context) {
    final trend = _trend;
    return Scaffold(
      appBar: AppBar(
        title: Text('${_today.year} so far'),
        actions: [
          IconButton(
            tooltip: 'Read again',
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
                  if (trend != null) ...[
                    _headline(trend),
                    const SizedBox(height: 16),
                    for (final month in trend.months)
                      _row(month, trend.largest),
                    const SizedBox(height: 16),
                    _footnote(trend),
                  ],
                ],
              ),
            ),
    );
  }

  Widget _headline(YearTrend trend) {
    final theme = Theme.of(context);
    if (trend.read.isEmpty) {
      return Text(
        'Not one month of ${_today.year} could be read, so there is nothing to say about the year yet.',
        style: theme.textTheme.titleMedium,
      );
    }
    final best = trend.best;
    final worst = trend.worst;
    final losing = trend.losing.length;
    final parts = <String>[];
    if (trend.isWhole) {
      final loss = trend.profit < ExactAmount.zero;
      parts.add(
        'The year so far: ${trend.income.toDecimalString()} in, '
        '${trend.expenses.toDecimalString()} out, '
        '${loss ? 'a loss of ${trend.profit.negated().toDecimalString()}' : 'a profit of ${trend.profit.toDecimalString()}'}.',
      );
    } else {
      parts.add(
        '${trend.read.length} of ${trend.months.length} months read, so the year '
        'total is withheld.',
      );
    }
    if (best != null) {
      parts.add('${best.name} was the strongest at ${_said(best.profit!)}.');
    }
    if (worst != null && worst != best) {
      parts.add('${worst.name} was the weakest at ${_said(worst.profit!)}.');
    }
    if (losing > 0) {
      parts.add(
        '$losing ${losing == 1 ? 'month lost money' : 'months lost money'}.',
      );
    }
    return Text(parts.join(' '), style: theme.textTheme.titleMedium);
  }

  /// A profit as a person says it: a loss is "400.00 lost", not "minus 400.00".
  String _said(ExactAmount figure) => figure < ExactAmount.zero
      ? '${figure.negated().toDecimalString()} lost'
      : figure.toDecimalString();

  /// One month: its name, a bar drawn to the year's largest figure, and what came in, went out and stayed.
  Widget _row(MonthFigure month, ExactAmount largest) {
    final theme = Theme.of(context);
    if (!month.wasRead) {
      return ListTile(
        contentPadding: EdgeInsets.zero,
        title: Text(month.name),
        subtitle: const Text('This month could not be read.'),
        trailing: const Icon(Icons.cloud_off),
      );
    }
    final fraction = largest.isZero
        ? 0.0
        : (month.income!.toDoubleLossy() / largest.toDoubleLossy()).clamp(
            0.0,
            1.0,
          );
    final loss = month.profit! < ExactAmount.zero;
    return InkWell(
      onTap: () => Navigator.of(context).push(
        MaterialPageRoute<void>(builder: (_) => const IncomeStatementPage()),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 10),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                SizedBox(width: 92, child: Text(month.name)),
                Expanded(
                  child: Text(
                    month.wasQuiet
                        ? 'a quiet month'
                        : '${month.income!.toDecimalString()} in, '
                              '${month.expenses!.toDecimalString()} out',
                    style: theme.textTheme.bodySmall,
                  ),
                ),
                Text(
                  month.wasQuiet ? '' : month.profit!.toDecimalString(),
                  style: theme.textTheme.bodyMedium?.copyWith(
                    fontWeight: FontWeight.bold,
                    color: loss ? theme.colorScheme.error : null,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            ClipRRect(
              borderRadius: BorderRadius.circular(3),
              child: LinearProgressIndicator(
                value: fraction,
                minHeight: 6,
                backgroundColor: theme.colorScheme.surfaceContainerHighest,
                color: loss
                    ? theme.colorScheme.error
                    : theme.colorScheme.primary,
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _footnote(YearTrend trend) {
    if (trend.unread == 0) {
      return Text(
        'Each month is the engine\'s own income statement for that month, and only posted '
        'entries move it: work left in draft reads as a quiet month. Tap one to see the lines behind it.',
        style: Theme.of(context).textTheme.bodySmall,
      );
    }
    return Text(
      '${trend.unread} month${trend.unread == 1 ? '' : 's'} could not be read, '
      'so the year total is withheld rather than counted from the rest.',
      style: Theme.of(context).textTheme.bodySmall,
    );
  }
}
