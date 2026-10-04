import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/finance_api_service.dart';
import 'package:vimbai_mobile_client/models/finance_models.dart';

class BudgetVarianceReportPage extends StatefulWidget {
  final String budgetId;
  final String budgetName;
  const BudgetVarianceReportPage({super.key, required this.budgetId, required this.budgetName});

  @override
  State<BudgetVarianceReportPage> createState() => _BudgetVarianceReportPageState();
}

class _BudgetVarianceReportPageState extends State<BudgetVarianceReportPage> {
  final FinanceApiService _apiService = FinanceApiService();
  late Future<VarianceAnalysisResult> _varianceFuture;

  @override
  void initState() {
    super.initState();
    _varianceFuture = _apiService.getBudgetVarianceAnalysis(widget.budgetId);
  }

  void _refresh() {
    setState(() {
      _varianceFuture = _apiService.getBudgetVarianceAnalysis(widget.budgetId);
    });
  }

  Widget _buildTotalRow(String label, double value, {Color? color}) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4.0),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(label, style: const TextStyle(fontSize: 16)),
          Text(
            value.toStringAsFixed(2),
            style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold, color: color),
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
      body: FutureBuilder<VarianceAnalysisResult>(
        future: _varianceFuture,
        builder: (context, snapshot) {
          if (snapshot.connectionState == ConnectionState.waiting) {
            return const Center(child: CircularProgressIndicator());
          } else if (snapshot.hasError) {
            return Center(child: Text('Error: ${snapshot.error}'));
          } else if (!snapshot.hasData) {
            return const Center(child: Text('No variance analysis available.'));
          }
          final result = snapshot.data!;
          return SingleChildScrollView(
            padding: const EdgeInsets.all(16.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Budget: ${result.budgetName}',
                    style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                const SizedBox(height: 12),
                _buildTotalRow('Total Variance', result.totalVariance),
                _buildTotalRow('Favorable Variance', result.favorableVariance, color: Colors.green),
                _buildTotalRow('Unfavorable Variance', result.unfavorableVariance, color: Colors.red),
                const SizedBox(height: 20),
                if (result.itemsOutsideTolerance.isEmpty)
                  const Text('All items are within tolerance.',
                      style: TextStyle(fontStyle: FontStyle.italic))
                else ...[
                  const Text('Items Outside Tolerance',
                      style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                  const Divider(),
                  ...result.itemsOutsideTolerance.map(
                    (item) => ListTile(
                      dense: true,
                      contentPadding: EdgeInsets.zero,
                      title: Text(item.description.isEmpty ? item.accountId : '${item.description} (${item.accountId})'),
                      subtitle: Text(
                          'Budget: ${item.budget.toStringAsFixed(2)} | Actual: ${item.actual.toStringAsFixed(2)}'),
                      trailing: Text(
                        '${item.variance.toStringAsFixed(2)} (${item.variancePct.toStringAsFixed(1)}%)',
                        style: TextStyle(
                          fontWeight: FontWeight.bold,
                          color: item.variance < 0 ? Colors.green : Colors.red,
                        ),
                      ),
                    ),
                  ),
                ],
                if (result.rootCauses.isNotEmpty) ...[
                  const SizedBox(height: 20),
                  const Text('Root Causes',
                      style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                  const Divider(),
                  ...result.rootCauses.entries.map(
                    (e) => Text('${e.key}: ${e.value}'),
                  ),
                ],
                if (result.correctiveActions.isNotEmpty) ...[
                  const SizedBox(height: 20),
                  const Text('Corrective Actions',
                      style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                  const Divider(),
                  ...result.correctiveActions.map(
                    (a) => ListTile(dense: true, contentPadding: EdgeInsets.zero, leading: const Icon(Icons.check_circle_outline), title: Text(a)),
                  ),
                ],
              ],
            ),
          );
        },
      ),
    );
  }
}
