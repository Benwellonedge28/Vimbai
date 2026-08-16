import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/finance_api_service.dart';
import 'package:vimbai_mobile_client/models/finance_models.dart';
import 'package:intl/intl.dart';

class BudgetVarianceReportPage extends StatefulWidget {
  final String budgetId;
  final String budgetName;

  const BudgetVarianceReportPage({super.key, required this.budgetId, required this.budgetName});

  @override
  State<BudgetVarianceReportPage> createState() => _BudgetVarianceReportPageState();
}

class _BudgetVarianceReportPageState extends State<BudgetVarianceReportPage> {
  late Future<BudgetVarianceReport> _varianceReportFuture;
  final FinanceApiService _apiService = FinanceApiService();

  @override
  void initState() {
    super.initState();
    _varianceReportFuture = _apiService.getBudgetVarianceReport(widget.budgetId);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text('${widget.budgetName} Variance Report'),
      ),
      body: FutureBuilder<BudgetVarianceReport>(
        future: _varianceReportFuture,
        builder: (context, snapshot) {
          if (snapshot.connectionState == ConnectionState.waiting) {
            return const Center(child: CircularProgressIndicator());
          } else if (snapshot.hasError) {
            return Center(child: Text('Error: ${snapshot.error}'));
          } else if (!snapshot.hasData) {
            return const Center(child: Text('No variance report data found.'));
          } else {
            final report = snapshot.data!;
            return SingleChildScrollView(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Card(
                    child: Padding(
                      padding: const EdgeInsets.all(16.0),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Budget: ${report.budgetName}',
                            style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
                          ),
                          const SizedBox(height: 8),
                          Text(
                            'Period: ${DateFormat.yMMMd().format(report.startDate)} - ${DateFormat.yMMMd().format(report.endDate)}',
                            style: const TextStyle(fontSize: 14),
                          ),
                          const SizedBox(height: 4),
                          Text(
                            'Generated: ${DateFormat.yMMMd().format(report.generatedAt)}',
                            style: const TextStyle(fontSize: 12, color: Colors.grey),
                          ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(height: 16),
                  Card(
                    child: Padding(
                      padding: const EdgeInsets.all(16.0),
                      child: Column(
                        children: [
                          Row(
                            mainAxisAlignment: MainAxisAlignment.spaceAround,
                            children: [
                              _buildSummaryItem('Total Budgeted', report.totalBudgeted),
                              _buildSummaryItem('Total Actual', report.totalActual),
                              _buildSummaryItem('Variance', report.totalVariance),
                            ],
                          ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(height: 24),
                  const Text(
                    'Variance Details:',
                    style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 10),
                  if (report.varianceItems.isEmpty)
                    const Text('No variance items found.')
                  else
                    ...report.varianceItems.map((item) => Card(
                      margin: const EdgeInsets.symmetric(vertical: 4),
                      child: Padding(
                        padding: const EdgeInsets.all(12.0),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              item.category,
                              style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16),
                            ),
                            if (item.accountNumber.isNotEmpty) ...[
                              const SizedBox(height: 4),
                              Text('Account: ${item.accountNumber}', style: const TextStyle(fontSize: 14)),
                            ],
                            const Divider(),
                            Row(
                              mainAxisAlignment: MainAxisAlignment.spaceBetween,
                              children: [
                                _buildVarianceColumn('Budgeted', item.budgetedAmount),
                                _buildVarianceColumn('Actual', item.actualAmount),
                                _buildVarianceColumn('Variance', item.variance, isVariance: true),
                                _buildVarianceColumn('%', item.variancePercentage, isPercentage: true),
                              ],
                            ),
                          ],
                        ),
                      ),
                    )).toList(),
                ],
              ),
            );
          }
        },
      ),
    );
  }

  Widget _buildSummaryItem(String label, double value) {
    return Column(
      children: [
        Text(
          label,
          style: const TextStyle(fontSize: 12, color: Colors.grey),
        ),
        const SizedBox(height: 4),
        Text(
          '\$${value.toStringAsFixed(2)}',
          style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
        ),
      ],
    );
  }

  Widget _buildVarianceColumn(String label, double value, {bool isVariance = false, bool isPercentage = false}) {
    Color valueColor = Colors.black;
    if (isVariance && value != 0) {
      valueColor = value > 0 ? Colors.green : Colors.red;
    }

    String displayValue = isPercentage
        ? '${value.toStringAsFixed(1)}%'
        : '\$${value.toStringAsFixed(2)}';

    return Column(
      children: [
        Text(label, style: const TextStyle(fontSize: 10, color: Colors.grey)),
        const SizedBox(height: 2),
        Text(
          displayValue,
          style: TextStyle(fontSize: 14, fontWeight: FontWeight.bold, color: valueColor),
        ),
      ],
    );
  }
}