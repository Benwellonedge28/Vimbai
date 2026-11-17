// An empty Book should explain a budget, and a read that failed must not look like one.
//
// "No budgets found." reads as a fact about the Book. It was also what a person saw when the read itself failed, which
// is the same trap the statements had: a failure dressed as emptiness, with nothing to do about either.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/budgeting/data/finance_api_service.dart';
import 'package:vimbai_mobile_client/features/budgeting/models/finance_models.dart';
import 'package:vimbai_mobile_client/features/budgeting/presentation/budgets_page.dart';

/// An engine with nothing in it, or one that refuses to answer.
class _BudgetEngine extends FinanceApiService {
  _BudgetEngine({this.refuse = false});

  final bool refuse;
  int reads = 0;

  @override
  Future<List<Budget>> getBudgets({bool forceRemote = false}) async {
    reads++;
    if (refuse) throw Exception('the budget store is unreachable');
    return const [];
  }
}

void main() {
  testWidgets('an empty Book says what a budget is and offers the first', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(home: BudgetsPage(service: _BudgetEngine())),
    );
    await tester.pumpAndSettle();

    expect(find.text('No budgets yet'), findsOneWidget);
    expect(
      find.textContaining('sets out what you expect to earn and spend'),
      findsOneWidget,
    );
    expect(find.text('Plan the first budget'), findsOneWidget);
    expect(find.text('Plan a budget'), findsOneWidget);
  });

  testWidgets('a failed read is named, with the reason, not called empty', (
    tester,
  ) async {
    final engine = _BudgetEngine(refuse: true);
    await tester.pumpWidget(MaterialApp(home: BudgetsPage(service: engine)));
    await tester.pumpAndSettle();

    expect(
      find.text('Your budgets could not be read just now.'),
      findsOneWidget,
    );
    expect(
      find.textContaining('the budget store is unreachable'),
      findsOneWidget,
    );
    expect(find.text('No budgets yet'), findsNothing);

    await tester.tap(find.text('Try again'));
    await tester.pumpAndSettle();
    expect(engine.reads, 2);
  });
}
