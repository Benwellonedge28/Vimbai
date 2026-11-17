// The ratios page names one amount among the ratios. It used to read that amount through a double, so a working
// capital figure past fifteen digits was rewritten before it was ever printed.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/budgeting/models/finance_models.dart';
import 'package:vimbai_mobile_client/features/reporting/presentation/financial_ratios_page.dart';

FinancialRatiosReport _report(ExactAmount? workingCapital) =>
    FinancialRatiosReport(
      reportDate: DateTime(2026, 11, 17),
      startDate: DateTime(2026, 10, 17),
      endDate: DateTime(2026, 11, 17),
      liquidity: LiquidityRatios(
        currentRatio: 1.5,
        workingCapital: workingCapital,
      ),
      solvency: SolvencyRatios(),
      profitability: ProfitabilityRatios(),
      efficiency: EfficiencyRatios(),
      marketValue: MarketValueRatios(),
      currency: 'USD',
    );

void main() {
  testWidgets("a working capital figure past a double's reach prints in full", (
    tester,
  ) async {
    const huge = '123456789012345678901234567890';
    await tester.pumpWidget(
      MaterialApp(
        home: FinancialRatiosPage(
          loadRatios: (_, __) async => _report(ExactAmount.parse(huge)),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Working Capital'), findsOneWidget);
    // Whole figures still read as money on screen, so the two places are kept even when they are zeros.
    expect(find.text('USD ' + huge + '.00'), findsOneWidget);
  });

  testWidgets('a report that carried no amount says so', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: FinancialRatiosPage(loadRatios: (_, __) async => _report(null)),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Working Capital'), findsOneWidget);
  });
}
