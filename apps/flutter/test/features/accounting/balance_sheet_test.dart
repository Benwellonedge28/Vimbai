// What the business is worth: the reading of a balance sheet, and what the page says about it.
//
// The cases that matter are the ones a demo skips: a sheet that agrees, one that does not (with the exact gap, whichever side
// is ahead), a fresh Book with nothing on it that must not claim a check passed, and a failed read that must never look like
// an empty sheet.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/data/position.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/balance_sheet_page.dart';

BalanceSheetItem _row(String name, String amount) =>
    BalanceSheetItem(category: name, amount: ExactAmount.parse(amount));

BalanceSheet _sheet({
  List<BalanceSheetItem> assets = const [],
  List<BalanceSheetItem> liabilities = const [],
  List<BalanceSheetItem> equity = const [],
  String? totalAssets,
  String? totalLiabilities,
  String? totalEquity,
  String? totalLiabilitiesEquity,
}) {
  ExactAmount sum(List<BalanceSheetItem> rows) =>
      ExactAmount.sum(rows.map((row) => row.amount));
  return BalanceSheet(
    asOfDate: DateTime(2026, 11, 18),
    assets: assets,
    liabilities: liabilities,
    equity: equity,
    totalAssets: ExactAmount.parse(
      totalAssets ?? sum(assets).toDecimalString(),
    ),
    totalLiabilities: ExactAmount.parse(
      totalLiabilities ?? sum(liabilities).toDecimalString(),
    ),
    totalEquity: ExactAmount.parse(
      totalEquity ?? sum(equity).toDecimalString(),
    ),
    totalLiabilitiesEquity: ExactAmount.parse(
      totalLiabilitiesEquity ??
          (sum(liabilities) + sum(equity)).toDecimalString(),
    ),
  );
}

class _Served extends AccountingApiService {
  _Served(this.sheet);

  final BalanceSheet sheet;

  @override
  Future<BalanceSheet> getBalanceSheet(DateTime asOfDate) async => sheet;
}

class _Fails extends AccountingApiService {
  @override
  Future<BalanceSheet> getBalanceSheet(DateTime asOfDate) async =>
      throw Exception('the engine answered 503 while it restarts');
}

void main() {
  group('reading the sheet', () {
    test('a sheet whose two sides agree balances', () {
      final position = positionOf(
        _sheet(
          assets: [_row('Cash', '1200.00'), _row('Debtors', '300.00')],
          liabilities: [_row('Creditors', '500.00')],
          equity: [_row('Retained', '1000.00')],
        ),
      );
      expect(position.verdict, PositionVerdict.balances);
      expect(position.gap.isZero, isTrue);
      expect(position.assets.total, ExactAmount.parse('1500.00'));
      expect(position.headline, contains('agree'));
    });

    test('a sheet out by 250.00 says so, and by how much', () {
      final position = positionOf(
        _sheet(
          assets: [_row('Cash', '1000.00')],
          liabilities: [_row('Creditors', '500.00')],
          equity: [_row('Retained', '250.00')],
        ),
      );
      expect(position.verdict, PositionVerdict.outOfBalance);
      expect(position.gap, ExactAmount.parse('250.00'));
      expect(position.headline, contains('out by 250.00'));
    });

    test('the gap is a size, whichever side is ahead', () {
      final position = positionOf(
        _sheet(
          assets: [_row('Cash', '1000.00')],
          liabilities: [],
          equity: [_row('Retained', '1250.00')],
        ),
      );
      expect(position.verdict, PositionVerdict.outOfBalance);
      expect(position.gap, ExactAmount.parse('250.00'));
      expect(position.gap.negative, isFalse);
    });

    test('a fresh Book says nothing is recorded, not that a check passed', () {
      final position = positionOf(
        _sheet(
          assets: [_row('Cash', '0'), _row('Bank', '0')],
          liabilities: [_row('Creditors', '0')],
          equity: [_row('Retained', '0')],
        ),
      );
      expect(position.verdict, PositionVerdict.nothingRecorded);
      expect(position.headline, contains('Nothing is on the sheet yet'));
    });

    test('accounts holding nothing are counted, not listed', () {
      final position = positionOf(
        _sheet(
          assets: [
            _row('Cash', '900.00'),
            _row('Bank', '0'),
            _row('Equipment', '0'),
          ],
          equity: [_row('Retained', '900.00')],
        ),
      );
      expect(position.assets.carrying.map((row) => row.category), ['Cash']);
      expect(position.assets.silent, 2);
      expect(position.verdict, PositionVerdict.balances);
    });

    test('the largest account in a section leads', () {
      final position = positionOf(
        _sheet(
          assets: [_row('Petty', '10.00'), _row('Bank', '900.00')],
          equity: [_row('Retained', '910.00')],
        ),
      );
      expect(position.assets.carrying.first.category, 'Bank');
    });

    test(
      'the totals are the engine\'s, even when they disagree with the rows',
      () {
        // the engine's own total is what the sheet stands on: re-adding the rows here would give 1000.00
        final position = positionOf(
          _sheet(
            assets: [_row('Cash', '1000.00')],
            equity: [_row('Retained', '1000.00')],
            totalAssets: '1234.00',
          ),
        );
        expect(position.assets.total, ExactAmount.parse('1234.00'));
        expect(position.verdict, PositionVerdict.outOfBalance);
      },
    );
  });

  group('the page', () {
    Widget page(AccountingApiService service) => MaterialApp(
      home: BalanceSheetPage(asOf: DateTime(2026, 11, 18), service: service),
    );

    testWidgets('leads with the verdict and lists what is on the sheet', (
      tester,
    ) async {
      await tester.pumpWidget(
        page(
          _Served(
            _sheet(
              assets: [_row('Cash', '1200.00'), _row('Debtors', '300.00')],
              liabilities: [_row('Creditors', '500.00')],
              equity: [_row('Retained', '1000.00')],
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('The two sides agree'), findsOneWidget);
      expect(find.textContaining('Assets · 1500.00'), findsOneWidget);
      expect(find.text('Cash'), findsOneWidget);
      expect(find.textContaining('Everything owned'), findsOneWidget);
      expect(find.text('1500.00'), findsWidgets);
    });

    testWidgets('an unbalanced sheet names the gap and where to look', (
      tester,
    ) async {
      await tester.pumpWidget(
        page(
          _Served(
            _sheet(
              assets: [_row('Cash', '1000.00')],
              equity: [_row('Retained', '750.00')],
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('out by 250.00'), findsOneWidget);
      expect(find.textContaining('Books in order'), findsOneWidget);
    });

    testWidgets('a fresh Book does not show a tick', (tester) async {
      await tester.pumpWidget(
        page(
          _Served(
            _sheet(
              assets: [_row('Cash', '0')],
              equity: [_row('Retained', '0')],
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(
        find.textContaining('Nothing is on the sheet yet'),
        findsOneWidget,
      );
      expect(find.textContaining('must agree'), findsNothing);
    });

    testWidgets('a failed read is never an empty sheet', (tester) async {
      await tester.pumpWidget(page(_Fails()));
      await tester.pumpAndSettle();

      expect(find.byType(CouldNotRead), findsOneWidget);
      expect(find.textContaining('Nothing is on the sheet yet'), findsNothing);
    });
  });
}
