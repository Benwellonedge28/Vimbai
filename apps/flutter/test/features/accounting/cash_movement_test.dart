// Where the money went: the reading of a cash flow statement, and what the page says about it.
//
// The cases a demo skips: money that grew, money that fell, money that came in and went out in equal measure (which is not
// the same as a period with nothing in it), a fresh Book that must not read as a quiet month, the cash book disagreeing with
// the books, and a failed read that must never be drawn as a period with no movement.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/data/cash_movement.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/cash_flow_statement_page.dart';

Map<String, dynamic> _answer({
  String net = '0',
  String opening = '0',
  String closing = '0',
  String cashBook = '0',
  bool agrees = true,
  int unclassified = 0,
  List<Map<String, dynamic>> sections = const [],
}) => {
  'start_date': '2026-09-01T00:00:00.000',
  'end_date': '2026-11-18T00:00:00.000',
  'sections': sections,
  'net_change': net,
  'opening_cash': opening,
  'closing_cash': closing,
  'unclassified': unclassified,
  'cash_book_total': cashBook,
  'agrees_with_cash_book': agrees,
};

Map<String, dynamic> _section(
  String title, {
  List<List<String>> lines = const [],
  String net = '0',
  int quiet = 0,
}) => {
  'title': title,
  'lines': [
    for (final line in lines) {'description': line[0], 'amount': line[1]},
  ],
  'net': net,
  'quiet': quiet,
};

class _Served extends AccountingApiService {
  _Served(this.answer);

  final CashMovement answer;

  @override
  Future<CashMovement> getCashMovement(DateTime start, DateTime end) async =>
      answer;
}

class _Fails extends AccountingApiService {
  @override
  Future<CashMovement> getCashMovement(DateTime start, DateTime end) async =>
      throw Exception('the engine answered 503 while it restarts');
}

void main() {
  group('reading where the money went', () {
    test('cash that grew says so with both balances', () {
      final movement = CashMovement.fromJson(
        _answer(
          net: '2500',
          opening: '1000',
          closing: '3500',
          cashBook: '3500',
          sections: [
            _section(
              'Operating',
              lines: [
                ['Sales', '500'],
              ],
              net: '500',
            ),
            _section('Investing', net: '0'),
            _section(
              'Financing',
              lines: [
                ['Loan', '2000'],
              ],
              net: '2000',
            ),
          ],
        ),
      );

      expect(movement.verdict, MovementVerdict.grew);
      expect(
        movement.headline,
        'Cash grew by 2500.00 over this period, from 1000.00 to '
        '3500.00.',
      );
      expect(movement.moved, ExactAmount.parse('2500'));
    });

    test('cash that fell reports the size of the fall, not a minus sign', () {
      final movement = CashMovement.fromJson(
        _answer(
          net: '-300',
          opening: '1200',
          closing: '900',
          cashBook: '900',
          sections: [
            _section(
              'Operating',
              lines: [
                ['Rent', '-300'],
              ],
              net: '-300',
            ),
          ],
        ),
      );

      expect(movement.verdict, MovementVerdict.shrank);
      expect(
        movement.headline,
        'Cash fell by 300.00 over this period, from 1200.00 to '
        '900.00.',
      );
    });

    test('as much in as out is not the same as nothing happening', () {
      final movement = CashMovement.fromJson(
        _answer(
          opening: '500',
          closing: '500',
          cashBook: '500',
          sections: [
            _section(
              'Operating',
              lines: [
                ['Sales', '200'],
              ],
              net: '200',
            ),
            _section(
              'Financing',
              lines: [
                ['Owners', '-200'],
              ],
              net: '-200',
            ),
          ],
        ),
      );

      expect(movement.verdict, MovementVerdict.stoodStill);
      expect(movement.headline, contains('stood still'));
    });

    test('a period with no movement at all says so', () {
      final movement = CashMovement.fromJson(
        _answer(
          sections: [
            _section('Operating'),
            _section('Investing'),
            _section('Financing'),
          ],
        ),
      );

      expect(movement.verdict, MovementVerdict.nothingMoved);
      expect(movement.headline, contains('No money has moved'));
    });

    test('the sources inside a class come largest first', () {
      final movement = CashMovement.fromJson(
        _answer(
          net: '1200',
          opening: '0',
          closing: '1200',
          cashBook: '1200',
          sections: [
            _section(
              'Operating',
              lines: [
                ['Small sale', '200'],
                ['Big sale', '1000'],
              ],
              net: '1200',
              quiet: 3,
            ),
          ],
        ),
      );

      final operating = movement.sections.first;
      expect(operating.sources.first.description, 'Big sale');
      expect(operating.quiet, 3);
    });

    test('the cash book is mentioned only when it disagrees', () {
      final agrees = CashMovement.fromJson(
        _answer(net: '100', opening: '0', closing: '100', cashBook: '100'),
      );
      expect(agrees.cashBookNote, isNull);

      final differs = CashMovement.fromJson(
        _answer(
          net: '250',
          opening: '0',
          closing: '250',
          cashBook: '1200',
          agrees: false,
        ),
      );
      expect(
        differs.cashBookNote,
        'The books say 250.00 is held; the cash book says 1200.00.',
      );
    });

    test('an unclassifiable line is reported rather than swept in', () {
      final movement = CashMovement.fromJson(
        _answer(net: '100', closing: '100', cashBook: '100', unclassified: 2),
      );
      expect(movement.unclassifiedNote, contains('2 lines'));
    });
  });

  group('the page', () {
    Widget page(AccountingApiService service) => MaterialApp(
      home: CashFlowStatementPage(
        start: DateTime(2026, 9, 1),
        end: DateTime(2026, 11, 18),
        service: service,
      ),
    );

    testWidgets('leads with the movement and lists where it came from', (
      tester,
    ) async {
      await tester.pumpWidget(
        page(
          _Served(
            CashMovement.fromJson(
              _answer(
                net: '2500',
                opening: '1000',
                closing: '3500',
                cashBook: '3500',
                sections: [
                  _section(
                    'Operating',
                    lines: [
                      ['Sales', '500'],
                    ],
                    net: '500',
                  ),
                  _section('Investing', net: '0'),
                  _section(
                    'Financing',
                    lines: [
                      ['Loan', '2000'],
                    ],
                    net: '2000',
                  ),
                ],
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('Cash grew by 2500.00'), findsOneWidget);
      expect(find.text('Sales'), findsOneWidget);
      expect(find.text('Loan'), findsOneWidget);
      expect(find.text('Held at the end'), findsOneWidget);
    });

    testWidgets('a failed read is not drawn as a quiet period', (tester) async {
      await tester.pumpWidget(page(_Fails()));
      await tester.pumpAndSettle();

      expect(find.textContaining('No money has moved'), findsNothing);
      expect(find.textContaining('could not'), findsWidgets);
    });

    testWidgets('a period with nothing in it explains itself', (tester) async {
      await tester.pumpWidget(
        page(
          _Served(
            CashMovement.fromJson(
              _answer(
                sections: [
                  _section('Operating'),
                  _section('Investing'),
                  _section('Financing'),
                ],
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('No money has moved'), findsOneWidget);
      expect(find.textContaining('still in draft'), findsOneWidget);
      expect(find.text('Operating'), findsNothing);
    });

    testWidgets('the cash book disagreement is on the screen', (tester) async {
      await tester.pumpWidget(
        page(
          _Served(
            CashMovement.fromJson(
              _answer(
                net: '250',
                opening: '0',
                closing: '250',
                cashBook: '1200',
                agrees: false,
                sections: [
                  _section(
                    'Operating',
                    lines: [
                      ['Sales', '250'],
                    ],
                    net: '250',
                  ),
                ],
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('the cash book says 1200.00'), findsOneWidget);
    });
  });
}
