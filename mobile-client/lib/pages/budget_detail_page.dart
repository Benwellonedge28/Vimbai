import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import 'package:finacc_mobile_client/models/finance_models.dart';
import 'package:finacc_mobile_client/pages/budget_variance_report_page.dart';

class BudgetDetailPage extends StatelessWidget {
  final Budget budget;
  const BudgetDetailPage({super.key, required this.budget});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(budget.name),
        actions: [
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
            Row(
              children: [
                const Icon(Icons.calendar_today, size: 20),
                const SizedBox(width: 8),
                Text(
                  'Period: ${DateFormat.yMMMd().format(budget.startDate)} - ${DateFormat.yMMMd().format(budget.endDate)}',
                  style: const TextStyle(fontSize: 16),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                const Icon(Icons.attach_money, size: 20),
                const SizedBox(width: 8),
                Text(
                  'Currency: ${budget.currency}',
                  style: const TextStyle(fontSize: 16),
                ),
              ],
            ),
            if (budget.description != null) ...[
              const SizedBox(height: 8),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Icon(Icons.description, size: 20),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Description: ${budget.description}',
                      style: const TextStyle(fontSize: 16),
                    ),
                  ),
                ],
              ),
            ],
            const SizedBox(height: 8),
            Row(
              children: [
                Icon(budget.isSynced ? Icons.cloud_done : Icons.cloud_off, size: 20),
                const SizedBox(width: 8),
                Text(
                  'Status: ${budget.isSynced ? "Synced" : "Not Synced"}',
                  style: const TextStyle(fontSize: 16),
                ),
              ],
            ),
            const SizedBox(height: 24),
            const Text('Budget Items:', style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
            const SizedBox(height: 10),
            if (budget.items.isEmpty)
              const Text('No budget items found.')
            else
              ...budget.items.map((item) => Card(
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
                      const SizedBox(height: 4),
                      Text(
                        'Budgeted Amount: ${budget.currency} ${item.budgetedAmount.toStringAsFixed(2)}',
                        style: const TextStyle(fontSize: 14),
                      ),
                      if (item.accountNumber.isNotEmpty) ...[
                        const SizedBox(height: 4),
                        Text(
                          'Account: ${item.accountNumber}',
                          style: const TextStyle(fontSize: 14),
                        ),
                      ],
                      if (item.budgetType != null) ...[
                        const SizedBox(height: 4),
                        Text(
                          'Type: ${item.budgetType}',
                          style: const TextStyle(fontSize: 14, color: Colors.grey),
                        ),
                      ],
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