// The general ledger on the phone.
//
// The report is the Book itself over a period, so what these tests hold is the two ways such a screen lies: by adding
// figures up itself instead of printing the engine's, and by quietly hiding an account that did not move or a window that
// did not balance. Each test answers with a report written the way the engine writes one and checks what a person reads.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/data/general_ledger.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/general_ledger_page.dart';

const big = '123456789012345678901234567890.500000';
const bigShown = '123456789012345678901234567890.50';

/// An engine that answers the report, and remembers what it was asked for.
class _LedgerEngine extends AccountingApiService {
  _LedgerEngine({this.fails = false, this.balanced = true});

  final bool fails;
  final bool balanced;

  DateTime? askedStart;
  DateTime? askedEnd;
  int reads = 0;

  @override
  Future<GeneralLedgerReport> getGeneralLedger({
    DateTime? startDate,
    DateTime? endDate,
    String? accountNumber,
  }) async {
    reads++;
    askedStart = startDate;
    askedEnd = endDate;
    if (fails) throw Exception('the books are unavailable');
    return GeneralLedgerReport.fromJson({
      'start_date': startDate?.toIso8601String() ?? '',
      'end_date': endDate?.toIso8601String() ?? '',
      'accounts': [
        {
          'account_number': '1000',
          'account_name': 'Cash',
          'account_type': 'asset',
          'normal_balance': 'debit',
          'start_balance': big,
          'total_debit': '30.000000',
          'total_credit': '0.000000',
          'end_balance': '75.000000',
          'entries': [
            {
              'entry_id': 'e-2',
              'entry_date': '2026-03-05',
              'description': 'a sale',
              'debit': '30.000000',
              'credit': '0.000000',
              'balance': '75.000000',
              'source_module': 'Sales',
            },
          ],
        },
        {
          'account_number': '2000',
          'account_name': 'Loan',
          'account_type': 'liability',
          'normal_balance': 'credit',
          'start_balance': '0.000000',
          'total_debit': '0.000000',
          'total_credit': '0.000000',
          'end_balance': '0.000000',
          'entries': <Map<String, dynamic>>[],
        },
      ],
      'account_count': 2,
      'movement_count': 1,
      'total_debit': '30.000000',
      'total_credit': balanced ? '30.000000' : '0.000000',
      'is_balanced': balanced,
    });
  }
}

void main() {
  testWidgets('every account is listed with what it opened and closed on', (
    tester,
  ) async {
    final engine = _LedgerEngine();
    await tester.pumpWidget(
      MaterialApp(home: GeneralLedgerPage(service: engine)),
    );
    await tester.pumpAndSettle();

    expect(find.text('2 accounts, 1 movement'), findsOneWidget);
    expect(find.text('1000 Cash'), findsOneWidget);
    // a figure with more digits than a double can hold is printed exactly as the engine wrote it
    expect(find.textContaining(bigShown), findsOneWidget);
    expect(find.text('Opened $bigShown · closed 75.00'), findsOneWidget);
    // and an account that did not move is still an account of the Book
    expect(find.text('2000 Loan'), findsOneWidget);
    expect(find.text('Did not move in this window'), findsOneWidget);
  });

  testWidgets('an account is opened to show its movements', (tester) async {
    await tester.pumpWidget(
      MaterialApp(home: GeneralLedgerPage(service: _LedgerEngine())),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('1000 Cash'));
    await tester.pumpAndSettle();

    expect(find.text('a sale'), findsOneWidget);
    expect(find.text('Dr 30.00'), findsOneWidget);
    expect(find.text('2026-03-05 · balance 75.00'), findsOneWidget);
    expect(find.text('Opening balance'), findsOneWidget);
    expect(find.text('Closing balance'), findsOneWidget);
  });

  testWidgets('a window that does not balance is said out loud', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: GeneralLedgerPage(service: _LedgerEngine(balanced: false)),
      ),
    );
    await tester.pumpAndSettle();

    expect(
      find.text(
        'The window does not balance: the debits and the credits differ.',
      ),
      findsOneWidget,
    );
    expect(find.text('The window debits what it credits.'), findsNothing);
  });

  testWidgets('a balanced window says so', (tester) async {
    await tester.pumpWidget(
      MaterialApp(home: GeneralLedgerPage(service: _LedgerEngine())),
    );
    await tester.pumpAndSettle();

    expect(find.text('The window debits what it credits.'), findsOneWidget);
  });

  testWidgets('the period asked for is the period the engine is asked for', (
    tester,
  ) async {
    final engine = _LedgerEngine();
    await tester.pumpWidget(
      MaterialApp(
        home: GeneralLedgerPage(service: engine, today: DateTime(2026, 3, 15)),
      ),
    );
    await tester.pumpAndSettle();

    // the whole Book by default: no window is invented on the phone
    expect(engine.askedStart, isNull);
    expect(engine.askedEnd, isNull);
    expect(find.text('Everything the Book holds'), findsOneWidget);

    await tester.tap(find.text('This month'));
    await tester.pumpAndSettle();
    expect(engine.askedStart, DateTime(2026, 3));
    // the month ends on the day before the next one begins
    expect(engine.askedEnd, DateTime(2026, 3, 31));

    await tester.tap(find.text('This year'));
    await tester.pumpAndSettle();
    expect(engine.askedStart, DateTime(2026));
    expect(engine.askedEnd, DateTime(2026, 12, 31));
  });

  testWidgets('a report that could not be read is said out loud', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(home: GeneralLedgerPage(service: _LedgerEngine(fails: true))),
    );
    await tester.pumpAndSettle();

    expect(
      find.text('The general ledger could not be read just now.'),
      findsOneWidget,
    );
    expect(find.text('the books are unavailable'), findsOneWidget);
  });
}
