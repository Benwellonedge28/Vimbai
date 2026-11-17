import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/budgeting/data/finance_api_service.dart';
import 'package:vimbai_mobile_client/features/budgeting/models/finance_models.dart';
import 'package:vimbai_mobile_client/features/budgeting/presentation/budget_detail_page.dart';
import 'package:vimbai_mobile_client/features/budgeting/presentation/budget_form_page.dart'; // NEW import

class BudgetsPage extends StatefulWidget {
  const BudgetsPage({super.key, this.service});

  /// The engine this screen talks to. A test hands in its own so the screen can be read without a phone.
  final FinanceApiService? service;

  @override
  State<BudgetsPage> createState() => _BudgetsPageState();
}

class _BudgetsPageState extends State<BudgetsPage> {
  late final FinanceApiService _apiService =
      widget.service ?? FinanceApiService();

  List<Budget> _budgets = const [];
  bool _loading = true;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _refreshBudgets();
  }

  /// Read the list again.
  ///
  /// The result is held as state rather than handed to a FutureBuilder, so a read that fails the instant it starts still
  /// lands on the error branch instead of finishing before anything is listening to it.
  Future<void> _refreshBudgets() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final budgets = await _apiService.getBudgets();
      if (mounted) {
        setState(() {
          _budgets = budgets;
          _error = null;
        });
      }
    } catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  /// Open the form, then read the list again: a budget that was saved should be visible without a pull.
  Future<void> _planABudget() async {
    await Navigator.of(
      context,
    ).push(MaterialPageRoute(builder: (context) => const BudgetFormPage()));
    _refreshBudgets();
  }

  /// A budget is a plan for one Book, so the empty screen says what one is and offers the first.
  Widget _noBudgetsYet() {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text(
              'No budgets yet',
              style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
            ),
            const SizedBox(height: 8),
            Text(
              'A budget sets out what you expect to earn and spend over a period, so you can compare it with what '
              'actually happened. Plan one for this Book and its lines can be tracked against the real figures.',
              textAlign: TextAlign.center,
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 16),
            ElevatedButton.icon(
              onPressed: _planABudget,
              icon: const Icon(Icons.add),
              label: const Text('Plan the first budget'),
            ),
          ],
        ),
      ),
    );
  }

  /// The list, or the reason there is no list. A read that failed is never dressed as an empty Book.
  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'Your budgets',
        error: _error,
        onRetry: _refreshBudgets,
      );
    }
    if (_budgets.isEmpty) return _noBudgetsYet();
    return ListView.builder(
      itemCount: _budgets.length,
      itemBuilder: (context, index) {
        final budget = _budgets[index];
        return Card(
          margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
          child: ListTile(
            title: Text(budget.name),
            subtitle: Text(
              '${budget.startDate.toLocal().toString().split(' ')[0]} to '
              '${budget.endDate.toLocal().toString().split(' ')[0]} (${budget.currency})',
            ),
            onTap: () {
              Navigator.of(context).push(
                MaterialPageRoute(
                  builder: (context) => BudgetDetailPage(budget: budget),
                ),
              );
            },
          ),
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Budgets')),
      body: _body(),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _planABudget,
        icon: const Icon(Icons.add),
        label: const Text('Plan a budget'),
      ),
    );
  }
}
