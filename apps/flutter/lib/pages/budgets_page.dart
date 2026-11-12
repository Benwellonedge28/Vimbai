import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/finance_api_service.dart';
import 'package:vimbai_mobile_client/models/finance_models.dart';
import 'package:vimbai_mobile_client/pages/budget_detail_page.dart';
import 'package:vimbai_mobile_client/pages/budget_form_page.dart'; // NEW import

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

  // Helper to refresh budgets after a new one is created
  void _refreshBudgets() {
    setState(() {
      _budgetsFuture = _apiService.getBudgets();
    });
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
                        subtitle: Text('${budget.startDate.toLocal().toString().split(' ')[0]} to ${budget.endDate.toLocal().toString().split(' ')[0]} (${budget.currency})'),
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
            onPressed: () async {
              // Navigate to a page to create a new budget
              await Navigator.of(context).push(
                MaterialPageRoute(builder: (context) => const BudgetFormPage()),
              );
              _refreshBudgets(); // Refresh list after returning from form
            },
            child: const Icon(Icons.add),
          ),
        );
      }
    }
