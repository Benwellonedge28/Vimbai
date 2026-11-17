// A stranger has no way of knowing an account number.
//
// The Ledger screen used to be one text field asking for one: the person was expected to remember that Cash is 1000.
// The chart is already in the Book, so it is offered for picking, and a figure is printed as the engine wrote it.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/ledger_page.dart';

const big = '123456789012345678901234567890.500000';
const bigShown = '123456789012345678901234567890.50';

/// An engine that hands the screen a chart, and one account's history, without a phone.
class _LedgerEngine extends AccountingApiService {
  _LedgerEngine({this.chartFails = false, this.ledgerFails = false});

  final bool chartFails;
  final bool ledgerFails;

  @override
  Future<List<Account>> getAllAccounts() async {
    if (chartFails) throw Exception('the chart is unavailable');
    return [
      Account(
        accountNumber: '1000',
        accountName: 'Cash',
        accountType: 'asset',
        normalBalance: 'debit',
      ),
      Account(
        accountNumber: '4000',
        accountName: 'Sales',
        accountType: 'revenue',
        normalBalance: 'credit',
      ),
    ];
  }

  @override
  Future<LedgerAccountBalance> getLedgerAccountBalance(String number) async {
    if (ledgerFails) throw Exception('that account does not exist');
    return LedgerAccountBalance(
      accountNumber: '1000',
      accountName: 'Cash',
      accountType: 'asset',
      normalBalance: 'debit',
      currentBalance: ExactAmount.parse(big),
    );
  }
}

void main() {
  testWidgets('the chart is offered, so an account is picked by name', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(home: LedgerPage(service: _LedgerEngine())),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Pick from the chart of accounts'));
    await tester.pumpAndSettle();

    expect(find.text('1000 - Cash'), findsOneWidget);
    expect(find.text('4000 - Sales'), findsOneWidget);

    await tester.tap(find.text('1000 - Cash'));
    await tester.pumpAndSettle();

    // The account was read, and its figure is printed in full rather than rounded.
    expect(find.textContaining(bigShown), findsOneWidget);
    expect(find.text('1000 - Cash'), findsOneWidget);
  });

  testWidgets('asking without picking or typing says what to do', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(home: LedgerPage(service: _LedgerEngine())),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byIcon(Icons.search));
    await tester.pumpAndSettle();

    expect(
      find.text(
        'Pick an account, or type its number, before asking for its history.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('a refused account repeats the reason in plain words', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(home: LedgerPage(service: _LedgerEngine(ledgerFails: true))),
    );
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField), '9999');
    await tester.tap(find.byIcon(Icons.search));
    await tester.pumpAndSettle();

    expect(find.textContaining('that account does not exist'), findsOneWidget);
    expect(find.textContaining('Exception:'), findsNothing);
  });

  testWidgets('a chart that could not be read is said out loud', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(home: LedgerPage(service: _LedgerEngine(chartFails: true))),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Pick from the chart of accounts'));
    await tester.pumpAndSettle();

    expect(
      find.text('The chart of accounts could not be read just now.'),
      findsOneWidget,
    );
  });
}
