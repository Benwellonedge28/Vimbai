// What the rate did, told apart honestly.
//
// The page exists for one reason: a difference on something paid is money and a difference on something still held is not.
// What is pinned here is that the two never arrive as a single total a person could spend, that the entries behind each
// figure are shown, and that a period in which nothing moved says so rather than reading as a loss of nothing.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/exchange_differences.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/exchange_differences_page.dart';

/// Stands in for the engine's report, so the page can be exercised without a server.
class _FakeReading extends ExchangeDifferencesService {
  _FakeReading(this.reading) : super.fresh();

  final ExchangeDifferences reading;

  @override
  Future<ExchangeDifferences> forPeriod(DateTime from, DateTime to) async =>
      reading;
}

ExchangeDifferences _reading({
  required String settledGain,
  required String heldGain,
  required String heldLoss,
}) {
  final entries = <RateEntry>[
    if (settledGain != '0')
      RateEntry(
        date: '2026-11-18',
        reference: 'USD-1',
        description: 'Settlement of USD-1',
        settled: true,
        gain: ExactAmount.parse(settledGain),
        loss: ExactAmount.zero,
      ),
    if (heldGain != '0')
      RateEntry(
        date: '2026-11-30',
        reference: 'FX-2026-11',
        description: 'Restatement of foreign balances at 2026-11',
        settled: false,
        gain: ExactAmount.parse(heldGain),
        loss: ExactAmount.zero,
      ),
    if (heldLoss != '0')
      RateEntry(
        date: '2026-11-30',
        reference: 'FX-2026-12',
        description: 'Restatement of foreign balances at 2026-12',
        settled: false,
        gain: ExactAmount.zero,
        loss: ExactAmount.parse(heldLoss),
      ),
  ];
  return ExchangeDifferences(
    from: '2026-11-01',
    to: '2026-11-30',
    everything: GainLoss(
      gain: ExactAmount.parse(settledGain) + ExactAmount.parse(heldGain),
      loss: ExactAmount.parse(heldLoss),
      net: ExactAmount.parse('0'),
    ),
    settled: GainLoss(
      gain: ExactAmount.parse(settledGain),
      loss: ExactAmount.zero,
      net: ExactAmount.parse(settledGain),
    ),
    held: GainLoss(
      gain: ExactAmount.parse(heldGain),
      loss: ExactAmount.parse(heldLoss),
      net: ExactAmount.parse(heldGain),
    ),
    entries: entries,
  );
}

void main() {
  testWidgets('money in hand and a figure on paper are never one total', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: ExchangeDifferencesPage(
          from: DateTime(2026, 11, 1),
          to: DateTime(2026, 11, 30),
          service: _FakeReading(
            _reading(settledGain: '2000', heldGain: '1500', heldLoss: '0'),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // each kind says what it is worth, and neither is added to the other
    expect(find.text('Money in hand'), findsOneWidget);
    expect(find.text('Not money yet'), findsOneWidget);
    expect(find.text('Gain 2000.00'), findsOneWidget);
    expect(find.text('Gain 1500.00'), findsOneWidget);
    expect(
      find.text('Gain 3500.00'),
      findsNothing,
      reason: 'a settled gain and a held gain are not one figure',
    );
    // and the entries behind the figures are listed
    expect(find.text('USD-1'), findsOneWidget);
    expect(find.textContaining('settled'), findsWidgets);
  });

  testWidgets('a period where nothing moved on the rate says exactly that', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: ExchangeDifferencesPage(
          from: DateTime(2026, 11, 1),
          to: DateTime(2026, 11, 30),
          service: _FakeReading(
            _reading(settledGain: '0', heldGain: '0', heldLoss: '0'),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Nothing moved on the rate'), findsOneWidget);
    expect(
      find.textContaining('Loss'),
      findsNothing,
      reason: 'nothing happening is not a loss',
    );
  });
}
