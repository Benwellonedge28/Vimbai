import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/services/finance_api_service.dart';
import 'package:finacc_mobile_client/models/finance_models.dart';
import 'package:finacc_mobile_client/pages/budget_detail_page.dart'; // Assuming this page will be created

class BudgetsPage extends StatefulWidget {
  const BudgetsPage({super.key});

  @override
  State<BudgetsPage> createState() => _BudgetsPageState();
}

class _BudgetsPageState extends State<BudgetsPage> {
  late Future<List<Budget>> _budgetsFuture;
  final FinanceApiService _apiService = FinanceApiService();

  @override
  void initState() {
    super.initState();
    _budgetsFuture = _apiService.getBudgets();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Budgets'),
          ),
          body: FutureBuilder<List<Budget>>(
            future: _budgetsFuture,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              } else if (snapshot.hasError) {
                return Center(child: Text('Error: ${snapshot.error}'));
              } else if (!snapshot.hasData || snapshot.data!.isEmpty) {
                return const Center(child: Text('No budgets found.'));
              } else {
                return ListView.builder(
                  itemCount: snapshot.data!.length,
                  itemBuilder: (context, index) {
                    final budget = snapshot.data![index];
                    return Card(
                      margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                      child: ListTile(
                        title: Text(budget.name),
                        subtitle: Text('${budget.period} ${budget.fiscalYear} - Status: ${budget.status}'),
                        onTap: () {
                          // Navigate to budget detail page
                          Navigator.of(context).push(MaterialPageRoute(
                            builder: (context) => BudgetDetailPage(budget: budget),
                          ));
                        },
                      ),
                    );
                  },
                );
              }
            },
          ),
          floatingActionButton: FloatingActionButton(
            onPressed: () {
              // Navigate to a page to create a new budget
              ScaffoldMessenger.of(context).showSnackBar(
                const SnackBar(content: Text('Create Budget functionality not yet implemented.')),
              );
            },
            child: const Icon(Icons.add),
          ),
        );
      }
    }
