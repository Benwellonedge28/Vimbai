import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/models/finance_models.dart';
import 'package:finacc_mobile_client/pages/budget_variance_report_page.dart'; // NEW import

class BudgetDetailPage extends StatelessWidget {
  final Budget budget;
  const BudgetDetailPage({super.key, required this.budget});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: Text(budget.name),
            actions: [ // NEW
              IconButton(
                icon: const Icon(Icons.analytics),
                tooltip: 'View Variance Report',
                onPressed: () {
                  if (budget.id != null) {
                    Navigator.of(context).push(MaterialPageRoute(
                      builder: (context) => BudgetVarianceReportPage(budgetId: budget.id!, budgetName: budget.name),
                    ));
                  } else {
                    ScaffoldMessenger.of(context).showSnackBar(
                      const SnackBar(content: Text('Cannot view variance report for unsaved budget.')),
                    );
                  }
                },
              ),
            ],
          ),
          body: SingleChildScrollView(
            padding: const EdgeInsets.all(16.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Fiscal Year: ${budget.fiscalYear}', style: const TextStyle(fontSize: 18)),
                Text('Period: ${budget.period}', style: const TextStyle(fontSize: 18)),
                Text('Status: ${budget.status}', style: const TextStyle(fontSize: 18)),
                if (budget.description != null) Text('Description: ${budget.description}', style: const TextStyle(fontSize: 16)),
                const SizedBox(height: 20),
                const Text('Budget Items:', style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
                const SizedBox(height: 10),
                if (budget.items.isEmpty)
                  const Text('No budget items found.')
                else
                  ...budget.items.map((item) => Card(
                    margin: const EdgeInsets.symmetric(vertical: 4),
                    child: Padding(
                      padding: const EdgeInsets.all(8.0),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text('Category: ${item.category}', style: const TextStyle(fontWeight: FontWeight.bold)),
                          Text('Budgeted: \$${item.budgetedAmount.toStringAsFixed(2)}'),
                          Text('Actual: \$${item.actualAmount.toStringAsFixed(2)}'),
                          if (item.description != null) Text('Description: ${item.description}'),
                          if (item.accountNumber != null) Text('Account: ${item.accountNumber}'),
                          Text('Period: ${item.periodStart.toLocal().toString().split(' ')[0]} to ${item.periodEnd.toLocal().toString().split(' ')[0]}'),
                        ],
                      ),
                    ),
                  )).toList(),
              ],
            ),
          ),
        );
      }
    }
