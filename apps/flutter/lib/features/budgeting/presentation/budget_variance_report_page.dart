import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/budgeting/data/finance_api_service.dart';
import 'package:vimbai_mobile_client/features/budgeting/models/finance_models.dart';

class BudgetVarianceReportPage extends StatefulWidget {
  final String budgetId;
  final String budgetName;
  const BudgetVarianceReportPage({
    super.key,
    required this.budgetId,
    required this.budgetName,
    this.service,
  });

  /// The engine this screen talks to. A test hands in its own so the screen can be read without a phone.
  final FinanceApiService? service;

  @override
  State<BudgetVarianceReportPage> createState() =>
      _BudgetVarianceReportPageState();
}

class _BudgetVarianceReportPageState extends State<BudgetVarianceReportPage> {
  late final FinanceApiService _apiService =
      widget.service ?? FinanceApiService();

  VarianceAnalysisResult? _result;
  bool _loading = true;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _refresh();
  }

  /// Read the analysis again, holding the result as state so a failed read always reaches the error branch.
  Future<void> _refresh() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final result = await _apiService.getBudgetVarianceAnalysis(
        widget.budgetId,
      );
      if (mounted) setState(() => _result = result);
    } catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Widget _buildTotalRow(String label, ExactAmount value, {Color? color}) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4.0),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(label, style: const TextStyle(fontSize: 16)),
          Text(
            value.toDecimalString(),
            style: TextStyle(
              fontSize: 16,
              fontWeight: FontWeight.bold,
              color: color,
            ),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text('Variance: ${widget.budgetName}'),
        actions: [
          IconButton(icon: const Icon(Icons.refresh), onPressed: _refresh),
        ],
      ),
      body: _body(),
    );
  }

  /// The analysis, or the reason there is none to show.
  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'The variance report',
        error: _error,
        onRetry: _refresh,
      );
    }
    final result = _result;
    if (result == null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Text(
            'This budget has no comparison to show yet. Its lines are compared with what was actually recorded '
            'once figures exist for the accounts they name.',
            textAlign: TextAlign.center,
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ),
      );
    }
    return SingleChildScrollView(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Budget: ${result.budgetName}',
            style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
          ),
          const SizedBox(height: 12),
          _buildTotalRow('Total Variance', result.totalVariance),
          _buildTotalRow(
            'Favorable Variance',
            result.favorableVariance,
            color: Colors.green,
          ),
          _buildTotalRow(
            'Unfavorable Variance',
            result.unfavorableVariance,
            color: Colors.red,
          ),
          const SizedBox(height: 20),
          if (result.itemsOutsideTolerance.isEmpty)
            const Text(
              'All items are within tolerance.',
              style: TextStyle(fontStyle: FontStyle.italic),
            )
          else ...[
            const Text(
              'Items Outside Tolerance',
              style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
            ),
            const Divider(),
            ...result.itemsOutsideTolerance.map(
              (item) => ListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                title: Text(
                  item.description.isEmpty
                      ? item.accountId
                      : '${item.description} (${item.accountId})',
                ),
                subtitle: Text(
                  'Budget: ${item.budget.toDecimalString()} | Actual: ${item.actual.toDecimalString()}',
                ),
                trailing: Text(
                  '${item.variance.toDecimalString()} (${item.variancePct.toStringAsFixed(1)}%)',
                  style: TextStyle(
                    fontWeight: FontWeight.bold,
                    color: item.variance.negative ? Colors.green : Colors.red,
                  ),
                ),
              ),
            ),
          ],
          if (result.rootCauses.isNotEmpty) ...[
            const SizedBox(height: 20),
            const Text(
              'Root Causes',
              style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
            ),
            const Divider(),
            ...result.rootCauses.entries.map(
              (e) => Text('${e.key}: ${e.value}'),
            ),
          ],
          if (result.correctiveActions.isNotEmpty) ...[
            const SizedBox(height: 20),
            const Text(
              'Corrective Actions',
              style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
            ),
            const Divider(),
            ...result.correctiveActions.map(
              (a) => ListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                leading: const Icon(Icons.check_circle_outline),
                title: Text(a),
              ),
            ),
          ],
        ],
      ),
    );
  }
}
