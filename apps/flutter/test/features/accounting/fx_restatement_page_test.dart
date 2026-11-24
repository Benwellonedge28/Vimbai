// Restating the month, from the balances the books already hold.
//
// Two things are pinned here. The sign a balance carries: a debt owed to the business goes in positive, a debt it owes goes
// in negative, so one movement of the rate raises both and the engine can tell a gain from a loss. And the rate: the screen
// never invents one, so nothing is posted until a person gives it.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/fx_restatement.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/fx_restatement_page.dart';

/// Stands in for the engine: it holds what the books say and records what it was asked to restate.
class _FakeFx extends FxRestatementService {
  _FakeFx(this.holdings_) : super.fresh();

  final ForeignHoldings holdings_;
  final List<Map<String, Object>> asked = [];

  @override
  Future<ForeignHoldings> holdings() async => holdings_;

  @override
  Future<RestatementPosted> restate({
    required String entryDate,
    required String closingRate,
    required List<RestatementBalance> balances,
  }) async {
    asked.add({
      'entry_date': entryDate,
      'closing_rate': closingRate,
      'balances': [for (final balance in balances) balance.toJson()],
    });
    return const RestatementPosted(
      period: '2026-11',
      gain: ExactAmount.zero,
      loss: ExactAmount.zero,
      restated: 2,
      journalEntryId: 'je-1',
      note: '',
    );
  }
}

ForeignHoldings _books() => ForeignHoldings(
  currencies: [
    CurrencyHolding(
      currency: 'USD',
      units: ExactAmount.parse('1500.00'),
      carried: ExactAmount.parse('38250.00'),
      documents: [
        ForeignDocument(
          reference: 'INV-1',
          kind: 'sale',
          date: '2026-11-02',
          rate: ExactAmount.parse('25.5'),
          units: ExactAmount.parse('1000.00'),
          carried: ExactAmount.parse('25500.00'),
        ),
        ForeignDocument(
          reference: 'BILL-1',
          kind: 'bill',
          date: '2026-11-05',
          rate: ExactAmount.parse('25.5'),
          units: ExactAmount.parse('500.00'),
          carried: ExactAmount.parse('12750.00'),
        ),
      ],
    ),
  ],
  note: 'What the books recorded, not what is still owed.',
);

void main() {
  testWidgets(
    'nothing is posted until a rate is given, and both sides carry their sign',
    (tester) async {
      final fx = _FakeFx(_books());
      await tester.pumpWidget(
        MaterialApp(
          home: FxRestatementPage(
            service: fx,
            entryDate: DateTime(2026, 11, 30),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('USD · 2 documents'), findsOneWidget);

      // no rate, no restatement: the screen will not pick one for the person
      await tester.tap(find.textContaining('Restate at 2026-11-30'));
      await tester.pumpAndSettle();
      expect(fx.asked, isEmpty);
      expect(find.textContaining('Give a closing rate'), findsOneWidget);

      await tester.enterText(
        find.widgetWithText(TextField, 'What one USD is worth'),
        '27',
      );
      await tester.pumpAndSettle();
      // 1,000 at 27 is 27,000 against 25,500 carried: a gain of 1,500; the bill loses the same, so the two are shown as what
      // the currency did rather than netted away
      expect(find.textContaining('Would'), findsOneWidget);

      await tester.tap(find.textContaining('Restate at 2026-11-30'));
      await tester.pumpAndSettle();

      final sent = fx.asked.single;
      expect(
        sent['closing_rate'],
        '27.00',
        reason: 'the exact figure the person typed, in the money\'s places',
      );
      expect(sent['entry_date'], '2026-11-30');
      final balances = (sent['balances'] as List).cast<Map<String, Object>>();
      expect(balances.first['account'], '1100');
      expect(
        balances.first['units'],
        '1000.00',
        reason: 'a debt owed to us is positive',
      );
      expect(balances.last['account'], '2000');
      expect(
        balances.last['units'],
        '-500.00',
        reason:
            'a debt we owe is negative, so the same rate movement raises both',
      );
    },
  );
}
