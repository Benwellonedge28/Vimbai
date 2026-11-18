// Closing the month, held to what it must never do.
//
// The page offers one consequential action, so these tests are mostly about when it refuses: a month is never offered
// while a check has found something wrong, and a check that could not be made is not treated as clean. They also pin the
// reading itself - the month on offer is the one just gone, never the one being lived in - and that a refusal from the
// engine is shown in the engine's own words rather than swallowed.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/close/data/close_report.dart';
import 'package:vimbai_mobile_client/features/close/data/close_source.dart';
import 'package:vimbai_mobile_client/features/close/data/month_close.dart';
import 'package:vimbai_mobile_client/features/close/presentation/close_the_month_page.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';

/// A door answering canned answers, recording what was asked.
class _FakeDoor {
  _FakeDoor({
    this.months = const [],
    this.closeStatus = 200,
    this.refusal = '',
  });

  final List<String> months;
  final int closeStatus;
  final String refusal;
  final List<http.Request> sent = [];

  late final ClosedPeriodsService service = ClosedPeriodsService.fresh()
    ..useClient(
      MockClient((request) async {
        sent.add(request);
        if (request.method == 'POST') {
          if (closeStatus != 200) {
            return http.Response('{"error":"$refusal"}', closeStatus);
          }
          final body = jsonDecode(request.body) as Map<String, dynamic>;
          return http.Response(
            jsonEncode({
              'period': body['period'],
              'closes_on': body['closes_on'],
              'closed_by': body['closed_by'],
              'note': '',
            }),
            200,
          );
        }
        return http.Response(
          jsonEncode({
            'closed_months': months,
            'closed': [
              for (final month in months)
                {
                  'period': month,
                  'closes_on': '2026-12-01',
                  'closed_by': 'Samuel',
                  'note': '',
                },
            ],
          }),
          200,
        );
      }),
    );
}

/// The three checks, answered by hand.
class _FakeChecks implements BookCloseSource {
  _FakeChecks({this.balance, this.entries, this.check, this.broken = false});

  final TrialBalance? balance;
  final List<JournalEntry>? entries;
  final PayablesCheck? check;
  final bool broken;

  @override
  Future<TrialBalance> trialBalance() async {
    if (broken) throw Exception('the engine is unreachable');
    return balance!;
  }

  @override
  Future<List<JournalEntry>> journal() async {
    if (broken) throw Exception('the engine is unreachable');
    return entries!;
  }

  @override
  Future<PayablesCheck> suppliersControl() async {
    if (broken) throw Exception('the engine is unreachable');
    return check!;
  }
}

TrialBalance _balanced() => TrialBalance(
  reportDate: DateTime(2026, 11, 19),
  entries: const [],
  totalDebits: ExactAmount.parse('1000.00'),
  totalCredits: ExactAmount.parse('1000.00'),
  isBalanced: true,
);

PayablesCheck _agreeing() => PayablesCheck(
  controlAccount: ExactAmount.zero,
  supplierTotal: ExactAmount.zero,
  difference: ExactAmount.zero,
  reconciled: true,
);

/// The button inside the dialog, which shares its words with the one on the page behind it.
Finder _inDialog(String words) => find.descendant(
  of: find.byType(AlertDialog),
  matching: find.widgetWithText(FilledButton, words),
);

void main() {
  group('the reading', () {
    test(
      'the month on offer is the one just gone, never the one being lived in',
      () {
        final open = MonthClose(
          today: '2026-11-19',
          closedMonths: const [],
          closed: const [],
        );
        expect(open.currentMonth, '2026-11');
        expect(open.monthOnOffer, '2026-10');
        expect(open.headline, 'October 2026 is still open');
        expect(open.nothingClosed, isTrue);
      },
    );

    test(
      'a month that is closed is skipped back to the earliest one still open',
      () {
        final months = MonthClose(
          today: '2026-11-19',
          closedMonths: const ['2026-09', '2026-10'],
          closed: const [],
        );
        // September and October are done, so August is what a late close would be about
        expect(months.monthOnOffer, '2026-08');
        expect(months.isClosed('2026-10'), isTrue);
        expect(months.openFrom, '2026-11-01');
      },
    );

    test('the month after December is January of the next year', () {
      final months = MonthClose(
        today: '2027-01-04',
        closedMonths: const ['2026-12'],
        closed: const [],
      );
      expect(months.monthOnOffer, '2026-11');
      expect(months.openFrom, '2027-01-01');
      expect(monthBefore('2027-01'), '2026-12');
      expect(monthOf('2026-11-19'), '2026-11');
    });

    test('a month is named the way a person says it', () {
      expect(spokenMonth('2026-11'), 'November 2026');
      expect(spokenMonth('2026-01'), 'January 2026');
      expect(spokenMonth('nonsense'), 'nonsense');
    });

    test('a closed month carries who closed it and when', () {
      const closed = ClosedPeriod(
        period: '2026-10',
        closesOn: '2026-11-05',
        closedBy: 'Samuel',
        note: 'After the bank statement',
      );
      expect(closed.spoken, 'October 2026');
      expect(
        closed.trail,
        'Closed by Samuel on 2026-11-05. After the bank statement',
      );
    });
  });

  group('the page', () {
    testWidgets(
      'offers the month just gone and closes it once the checks are clean',
      (tester) async {
        final door = _FakeDoor();
        final checks = _FakeChecks(
          balance: _balanced(),
          entries: const [],
          check: _agreeing(),
        );

        await tester.pumpWidget(
          MaterialApp(
            home: CloseTheMonthPage(
              service: door.service,
              source: checks,
              today: '2026-11-19',
              closedBy: 'Samuel',
            ),
          ),
        );
        await tester.pumpAndSettle();

        expect(find.text('October 2026 is still open'), findsOneWidget);
        expect(
          find.text('No month has been closed yet, so the books are open.'),
          findsOneWidget,
        );

        await tester.tap(
          find.widgetWithText(FilledButton, 'Close October 2026'),
        );
        await tester.pumpAndSettle();

        // what closing means, said before anything is written
        expect(find.text('Close October 2026?'), findsOneWidget);
        await tester.tap(_inDialog('Close October 2026'));
        await tester.pumpAndSettle();

        final posted = door.sent
            .where((request) => request.method == 'POST')
            .toList();
        expect(posted, hasLength(1));
        final body = jsonDecode(posted.single.body) as Map<String, dynamic>;
        expect(body['period'], '2026-10');
        expect(body['closes_on'], '2026-11-19');
        expect(body['closed_by'], 'Samuel');
      },
    );

    testWidgets(
      'a draft in the journal withholds the close and names what to fix',
      (tester) async {
        final door = _FakeDoor();
        final checks = _FakeChecks(
          balance: _balanced(),
          entries: [
            JournalEntry(
              id: 'e-1',
              entryDate: DateTime(2026, 10, 20),
              description: 'late invoice',
              sourceModule: 'Manual',
              status: 'draft',
              lines: const [],
            ),
          ],
          check: _agreeing(),
        );

        await tester.pumpWidget(
          MaterialApp(
            home: CloseTheMonthPage(
              service: door.service,
              source: checks,
              today: '2026-11-19',
            ),
          ),
        );
        await tester.pumpAndSettle();

        final button = tester.widget<FilledButton>(
          find.widgetWithText(FilledButton, 'Close October 2026'),
        );
        expect(
          button.onPressed,
          isNull,
          reason: 'a month is not closed over a draft',
        );
        expect(find.textContaining('still in draft'), findsWidgets);
        expect(find.text('Open Books in order'), findsOneWidget);
        expect(door.sent.where((request) => request.method == 'POST'), isEmpty);
      },
    );

    testWidgets('a check that could not be made is never counted as clean', (
      tester,
    ) async {
      final door = _FakeDoor();
      final checks = _FakeChecks(broken: true);

      await tester.pumpWidget(
        MaterialApp(
          home: CloseTheMonthPage(
            service: door.service,
            source: checks,
            today: '2026-11-19',
          ),
        ),
      );
      await tester.pumpAndSettle();

      final button = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, 'Close October 2026'),
      );
      expect(button.onPressed, isNull);
      expect(find.textContaining('could not be made'), findsWidgets);
    });

    testWidgets('a month already closed is shown as closed, with the trail', (
      tester,
    ) async {
      final door = _FakeDoor(months: const ['2026-09', '2026-10']);
      final checks = _FakeChecks(
        balance: _balanced(),
        entries: const [],
        check: _agreeing(),
      );

      await tester.pumpWidget(
        MaterialApp(
          home: CloseTheMonthPage(
            service: door.service,
            source: checks,
            today: '2026-11-19',
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('October 2026'), findsOneWidget);
      expect(find.text('September 2026'), findsOneWidget);
      expect(find.text('Closed by Samuel on 2026-12-01'), findsNWidgets(2));
      // August is the earliest month still open, so that is what is on offer
      expect(find.textContaining('August 2026'), findsWidgets);
    });

    testWidgets('the engine refusal is shown in its own words', (tester) async {
      final door = _FakeDoor(
        closeStatus: 422,
        refusal:
            '2026-10 is already closed, so nothing more can be posted into it.',
      );
      final checks = _FakeChecks(
        balance: _balanced(),
        entries: const [],
        check: _agreeing(),
      );

      await tester.pumpWidget(
        MaterialApp(
          home: CloseTheMonthPage(
            service: door.service,
            source: checks,
            today: '2026-11-19',
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.widgetWithText(FilledButton, 'Close October 2026'));
      await tester.pumpAndSettle();
      await tester.tap(_inDialog('Close October 2026'));
      await tester.pumpAndSettle();

      expect(find.text('That month was not closed'), findsOneWidget);
      expect(
        find.text(
          '2026-10 is already closed, so nothing more can be posted into it.',
        ),
        findsOneWidget,
      );
    });

    testWidgets(
      'a failed read is reported rather than shown as nothing closed',
      (tester) async {
        final broken = ClosedPeriodsService.fresh()
          ..useClient(
            MockClient(
              (request) async => http.Response('{"detail":"no book"}', 500),
            ),
          );

        await tester.pumpWidget(
          MaterialApp(
            home: CloseTheMonthPage(
              service: broken,
              source: _FakeChecks(
                balance: _balanced(),
                entries: const [],
                check: _agreeing(),
              ),
              today: '2026-11-19',
            ),
          ),
        );
        await tester.pumpAndSettle();

        expect(
          find.text('The months closed could not be read just now.'),
          findsOneWidget,
        );
        expect(
          find.textContaining('No month has been closed yet'),
          findsNothing,
        );
      },
    );
  });
}
