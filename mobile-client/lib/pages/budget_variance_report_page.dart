import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/services/finance_api_service.dart';
import 'package:finacc_mobile_client/models/finance_models.dart';
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
                      Text('Report Date: ${DateFormat.yMMMd().format(report.reportDate)}',
                          style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                      Text('Fiscal Year: ${report.fiscalYear} | Period: ${report.period}'),
                      const SizedBox(height: 20),
                      const Text('Variance Details:', style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
                      const SizedBox(height: 10),
                      DataTable(
                        columnSpacing: 10,
                        horizontalMargin: 10,
                        columns: const [
                          DataColumn(label: Text('Category', style: TextStyle(fontWeight: FontWeight.bold))),
                          DataColumn(label: Text('Budgeted', style: TextStyle(fontWeight: FontWeight.bold)), numeric: true),
                          DataColumn(label: Text('Actual', style: TextStyle(fontWeight: FontWeight.bold)), numeric: true),
                          DataColumn(label: Text('Variance', style: TextStyle(fontWeight: FontWeight.bold)), numeric: true),
                          DataColumn(label: Text('%', style: TextStyle(fontWeight: FontWeight.bold)), numeric: true),
                        ],
                        rows: report.items.map((item) => DataRow(cells: [
                          DataCell(Text(item.category)),
                          DataCell(Text(item.budgetedAmount.toStringAsFixed(2))),
                          DataCell(Text(item.actualAmount.toStringAsFixed(2))),
                          DataCell(Text(item.variance.toStringAsFixed(2))),
                          DataCell(Text('${item.variancePercentage.toStringAsFixed(2)}%')),
                        ])).toList(),
                      ),
                      const Divider(),
                      Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 10.0, vertical: 5.0),
                        child: Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            const Text('TOTALS', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                            Text(report.totalBudgeted.toStringAsFixed(2), style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                            Text(report.totalActual.toStringAsFixed(2), style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                            Text(report.totalVariance.toStringAsFixed(2), style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                            Text('${report.totalVariancePercentage.toStringAsFixed(2)}%', style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                          ],
                        ),
                      ),
                    ],
                  ),
                );
              }
            },
          ),
        );
      }
    }
