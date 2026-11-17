import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/household_finance/data/household_finance_service.dart';
import 'package:vimbai_mobile_client/features/household_finance/presentation/household_finance_page.dart';

/// A client that records what the page asks for instead of sending it, so the test can read the figures the engine
/// would have received.
class _RecordingService extends HouseholdFinanceService {
  _RecordingService() : super.forTest();

  late Map<String, dynamic> request;

  @override
  Future<Map<String, dynamic>> analyze({
    required String householdId,
    required String period,
    List<Map<String, dynamic>> incomes = const [],
    List<Map<String, dynamic>> expenses = const [],
    List<Map<String, dynamic>> assets = const [],
    List<Map<String, dynamic>> liabilities = const [],
  }) async {
    request = {
      'household_id': householdId,
      'period': period,
      'incomes': incomes,
      'expenses': expenses,
      'assets': assets,
      'liabilities': liabilities,
    };
    return const {
      'total_income': '0.00',
      'total_expenses': '0.00',
      'surplus_deficit': '0.00',
      'savings_rate': 0.0,
      'net_worth': '0.00',
      'budget_health': 'fair',
      'expense_by_category': <String, dynamic>{},
    };
  }
}

/// Fill the dialog the page opens and confirm it.
Future<void> _fillDialog(WidgetTester tester, List<String> texts) async {
  // Only the dialog's own fields: the page behind it has fields of its own (the household and its period).
  final fields = find.descendant(
    of: find.byType(AlertDialog),
    matching: find.byType(TextField),
  );
  for (var i = 0; i < texts.length; i++) {
    await tester.enterText(fields.at(i), texts[i]);
  }
  await tester.tap(find.widgetWithText(ElevatedButton, 'Add'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('an income larger than a double is sent as its own digits', (
    tester,
  ) async {
    final service = _RecordingService();
    await tester.pumpWidget(
      MaterialApp(home: HouseholdFinancePage(service: service)),
    );
    await tester.pumpAndSettle();

    const huge = '123456789012345678901234567890.123456';
    await tester.tap(find.text('Add income'));
    await tester.pumpAndSettle();
    await _fillDialog(tester, ['salary', huge]);

    await tester.tap(find.text('Analyze household'));
    await tester.pumpAndSettle();

    final incomes = service.request['incomes'] as List;
    expect(incomes, hasLength(1));
    expect(
      incomes.first['amount'],
      huge,
      reason: 'the figure travels as text, so no double can rewrite its digits',
    );
  });

  testWidgets('a figure that is not a number is refused, not sent as zero', (
    tester,
  ) async {
    final service = _RecordingService();
    await tester.pumpWidget(
      MaterialApp(home: HouseholdFinancePage(service: service)),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Add income'));
    await tester.pumpAndSettle();
    await _fillDialog(tester, ['salary', 'twelve']);

    expect(find.text('Enter a source and a positive amount'), findsOneWidget);
    await tester.tap(find.text('Analyze household'));
    await tester.pumpAndSettle();
    expect(
      (service.request['incomes'] as List),
      isEmpty,
      reason: 'nothing was added, so nothing is sent',
    );
  });
}
