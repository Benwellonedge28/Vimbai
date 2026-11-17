// A ratios screen must name a read that failed, and must not call an empty period an empty report.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/budgeting/models/finance_models.dart';
import 'package:vimbai_mobile_client/features/reporting/presentation/financial_ratios_page.dart';

void main() {
  testWidgets('a failed read is named, with the reason, not called empty', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: FinancialRatiosPage(
          loadRatios: (_, __) async => throw Exception('the engine refused'),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(
      find.text('The financial ratios could not be read just now.'),
      findsOneWidget,
    );
    expect(find.textContaining('the engine refused'), findsOneWidget);
    expect(find.textContaining('No ratios can be worked out'), findsNothing);
  });

  testWidgets('an empty period says what ratios are made from', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: FinancialRatiosPage(loadRatios: (_, __) async => _emptyReport()),
      ),
    );
    await tester.pumpAndSettle();

    expect(
      find.textContaining('No ratios can be worked out for this period yet'),
      findsOneWidget,
    );
  });
}

/// A report with every reading absent, which is what an empty period comes back as.
///
/// The engine answers a report either way, so "empty" is not a missing report: it is a report whose readings are all
/// absent, which is why the page asks the model rather than checking for null.
FinancialRatiosReport _emptyReport() => FinancialRatiosReport(
  reportDate: DateTime(2026, 11, 17),
  startDate: DateTime(2026, 10, 1),
  endDate: DateTime(2026, 11, 17),
  liquidity: LiquidityRatios(),
  solvency: SolvencyRatios(),
  profitability: ProfitabilityRatios(),
  efficiency: EfficiencyRatios(),
  marketValue: MarketValueRatios(),
  currency: 'USD',
);
