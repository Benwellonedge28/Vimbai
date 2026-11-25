// How am I doing: the month so far, read as a decision.
//
// Three things are pinned here, because each is a way the screen could lie to somebody deciding something:
//
// * the figure is the engine's. A phone that worked out its own profit would disagree with the ledger it reports on.
// * a part month is compared like for like, and the screen says which days it compared against.
// * what is not posted is said out loud. A provisional profit presented as final is worse than no figure at all.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/progress/data/measures_reading.dart';
import 'package:vimbai_mobile_client/features/progress/data/progress_reading.dart';
import 'package:vimbai_mobile_client/features/progress/presentation/how_am_i_doing_page.dart';

const _october = '''
{"from":"2026-10-01","to":"2026-10-20","days":20,
 "revenue":"1150.000000","costs":"150.000000","profit":"1000.000000","margin_percent":"86.960000",
 "compared_with":{"from":"2026-09-01","to":"2026-09-20","revenue":"0.000000","costs":"0.000000",
                  "profit":"0.000000"},
 "change":{"revenue":"1150.000000","costs":"150.000000","profit":"1000.000000"},
 "cash":{"total":"250.000000","accounts":[
   {"name":"Trading account","kind":"bank","balance":"250.000000"}]},
 "owed_to_you":"1150.000000","owed_by_you":"460.000000",
 "tax":{"owed":"90.000000","accounts":1},
 "not_posted":{"drafts":0,"oldest":null},
 "note":"Every entry in this window is posted, so the figures are the ledger's own."}
''';

const _provisional = '''
{"from":"2026-10-01","to":"2026-10-20","days":20,
 "revenue":"1150.000000","costs":"150.000000","profit":"1000.000000","margin_percent":"86.960000",
 "compared_with":{"from":"2026-09-01","to":"2026-09-20","revenue":"0.000000","costs":"0.000000",
                  "profit":"0.000000"},
 "change":{"revenue":"1150.000000","costs":"150.000000","profit":"1000.000000"},
 "cash":{"total":"0.000000","accounts":[]},
 "owed_to_you":"0.000000","owed_by_you":"0.000000",
 "tax":{"owed":"0.000000","accounts":0},
 "not_posted":{"drafts":2,"oldest":"2026-10-12"},
 "note":"2 entries in this window have not been posted, so the profit here is provisional."}
''';

const _nothing = '''
{"from":"2026-10-01","to":"2026-10-20","days":20,
 "revenue":"0.000000","costs":"0.000000","profit":"0.000000","margin_percent":"0.000000",
 "compared_with":{"from":"2026-09-01","to":"2026-09-20","revenue":"0.000000","costs":"0.000000",
                  "profit":"0.000000"},
 "change":{"revenue":"0.000000","costs":"0.000000","profit":"0.000000"},
 "cash":{"total":"0.000000","accounts":[]},
 "owed_to_you":"0.000000","owed_by_you":"0.000000","tax":{"owed":"0.000000","accounts":0},
 "not_posted":{"drafts":0,"oldest":null},
 "note":"Every entry in this window is posted, so the figures are the ledger's own."}
''';

/// The Book's own figures, as the engine answers them: rows in the Book's order, one leading.
const _figures = '''
{"from":"2026-10-01","to":"2026-10-20","measures":[
 {"name":"profit","label":"Profit","unit":"money","value":"1000.000000","leads":true},
 {"name":"cash","label":"Cash held","unit":"money","value":"800.000000","leads":true},
 {"name":"margin","label":"Profit as a share of trading","unit":"percent","value":"86.956522","leads":false},
 {"name":"not_posted","label":"Entries not yet posted","unit":"count","value":2,"leads":false}]}
''';

const _figuresWithAProblem = '''
{"from":"2026-10-01","to":"2026-10-20","measures":[
 {"name":"profit","label":"Profit","unit":"money","value":"1000.000000","leads":true},
 {"name":"mine","label":"What my own measure counts","unit":"money","value":null,
  "problem":"this Book has no account called Depot costs","leads":false}]}
''';

BookMeasuresService _measuresReturning(String body, {int status = 200}) {
  final service = BookMeasuresService.fresh();
  service.useClient(MockClient((request) async => http.Response(body, status)));
  service.useToken('token');
  return service;
}

ProgressService _serviceReturning(
  String body, {
  int status = 200,
  List<Uri>? asked,
}) {
  final service = ProgressService.fresh();
  service.useClient(
    MockClient((request) async {
      asked?.add(request.url);
      return http.Response(body, status, headers: const {});
    }),
  );
  service.setAuthToken('token');
  return service;
}

Future<void> _open(
  WidgetTester tester,
  ProgressService service, {
  String figures = _figures,
}) async {
  // a tall surface so the whole page is built: the screen is longer than a default test viewport
  tester.view.physicalSize = const Size(1200, 3200);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      home: HowAmIDoingPage(
        service: service,
        measures: _measuresReturning(figures),
        today: '2026-10-20',
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  group('the reading', () {
    test(
      'a reading is the engine\'s own figures, not ones worked out here',
      () {
        final reading = ProgressReading.fromJson(
          jsonDecode(_october) as Map<String, dynamic>,
        );
        expect(reading.revenue, '1150.000000');
        expect(reading.costs, '150.000000');
        expect(reading.profit, '1000.000000');
        expect(reading.marginPercent, '86.960000');
        expect(reading.days, 20);
        expect(reading.cashAccounts.single.name, 'Trading account');
        expect(reading.owedToYou, '1150.000000');
        expect(reading.taxOwed, '90.000000');
        expect(reading.isProvisional, isFalse);
      },
    );

    test(
      'a part month is compared with the days it names, never with a whole one',
      () {
        final reading = ProgressReading.fromJson(
          jsonDecode(_october) as Map<String, dynamic>,
        );
        expect(reading.comparedFrom, '2026-09-01');
        expect(reading.comparedTo, '2026-09-20');
        expect(reading.comparedWindow, '1 September 2026 - 20 September 2026');
        expect(reading.window, '1-20 October 2026');
      },
    );

    test('a window that spans two months says both', () {
      final reading = ProgressReading.fromJson({
        ...jsonDecode(_october) as Map<String, dynamic>,
        'from': '2026-10-05',
        'to': '2026-11-08',
      });
      expect(reading.window, '5 October 2026 - 8 November 2026');
    });

    test('unposted entries make the reading provisional', () {
      final reading = ProgressReading.fromJson(
        jsonDecode(_provisional) as Map<String, dynamic>,
      );
      expect(reading.isProvisional, isTrue);
      expect(reading.drafts, 2);
      expect(reading.oldestDraft, '2026-10-12');
    });

    test(
      'more made than held is noticed, so cash and profit are never merged',
      () {
        final reading = ProgressReading.fromJson(
          jsonDecode(_october) as Map<String, dynamic>,
        );
        expect(reading.madeMoreThanItHolds, isTrue);
      },
    );

    test(
      'the window asked for is the month so far unless a day is named',
      () async {
        final asked = <Uri>[];
        await _serviceReturning(_october, asked: asked).read(to: '2026-10-20');
        expect(asked.single.queryParameters['to'], '2026-10-20');
        expect(asked.single.queryParameters.containsKey('from'), isFalse);
        expect(asked.single.path, endsWith('/progress'));
      },
    );

    test('a window is sent when one is named', () async {
      final asked = <Uri>[];
      await _serviceReturning(
        _october,
        asked: asked,
      ).read(to: '2026-10-20', from: '2026-10-05');
      expect(asked.single.queryParameters['from'], '2026-10-05');
    });

    test('a refusal is read in the engine\'s own words', () async {
      final service = _serviceReturning(
        '{"detail":"Say the day to report on, as to=2026-10-20."}',
        status: 422,
      );
      await expectLater(
        service.read(to: '2026-10-20'),
        throwsA(
          predicate(
            (error) => error.toString().contains('Say the day to report on'),
          ),
        ),
      );
    });
  });

  group('the screen', () {
    testWidgets('it opens on today and leads with what has been made', (
      tester,
    ) async {
      await _open(tester, _serviceReturning(_october));
      // the figures the Book leads with, in the Book's own words, rather than a sentence naming profit and margin
      expect(find.text('Profit: 1000.00'), findsOneWidget);
      expect(find.text('Cash held: 800.00'), findsOneWidget);
      expect(
        find.textContaining('What you measure yourself by'),
        findsOneWidget,
      );
      expect(find.text('Profit as a share of trading'), findsOneWidget);
      expect(find.textContaining('86.9'), findsOneWidget);
    });

    testWidgets(
      'it says what it compared against, so a part month is not mistaken for a whole one',
      (tester) async {
        await _open(tester, _serviceReturning(_october));
        expect(
          find.textContaining('Against the same days before'),
          findsOneWidget,
        );
        expect(
          find.textContaining('over 1 September 2026 - 20 September 2026'),
          findsOneWidget,
        );
        expect(find.textContaining('1000.00 ahead'), findsOneWidget);
      },
    );

    testWidgets(
      'cash is shown beside profit, with each account that holds it',
      (tester) async {
        await _open(tester, _serviceReturning(_october));
        expect(
          find.textContaining(
            '250.00 in the accounts you keep, against 1000.00 made',
          ),
          findsOneWidget,
        );
        expect(find.text('Trading account'), findsOneWidget);
        expect(
          find.textContaining('More has been made than is in the bank'),
          findsOneWidget,
        );
      },
    );

    testWidgets(
      'what is owed each way and the tax set aside are on the same screen',
      (tester) async {
        await _open(tester, _serviceReturning(_october));
        await tester.scrollUntilVisible(find.text('Owed to you'), 200);
        expect(find.text('Owed to you'), findsOneWidget);
        expect(find.text('1150.00'), findsOneWidget);
        expect(find.text('Owed by you'), findsOneWidget);
        expect(find.text('Tax set aside'), findsOneWidget);
        expect(find.text('90.00'), findsOneWidget);
      },
    );

    testWidgets(
      'unposted entries are said out loud rather than left to be discovered',
      (tester) async {
        await _open(tester, _serviceReturning(_provisional));
        await tester.scrollUntilVisible(
          find.text('2 entries in this window are not posted'),
          200,
        );
        expect(
          find.text('2 entries in this window are not posted'),
          findsOneWidget,
        );
        expect(find.textContaining('dated 2026-10-12'), findsOneWidget);
        expect(find.textContaining('provisional'), findsWidgets);
      },
    );

    testWidgets(
      'an empty Book explains itself instead of showing zeroes as if it had traded',
      (tester) async {
        await _open(tester, _serviceReturning(_nothing));
        expect(
          find.text('Nothing has been posted in this window yet'),
          findsOneWidget,
        );
        expect(find.textContaining('without closing anything'), findsOneWidget);
      },
    );

    testWidgets(
      'a measure that cannot be read says why, and the others still show',
      (tester) async {
        await _open(
          tester,
          _serviceReturning(_october),
          figures: _figuresWithAProblem,
        );
        expect(find.text('What my own measure counts'), findsOneWidget);
        expect(find.text('could not be read'), findsOneWidget);
        expect(
          find.textContaining('no account called Depot costs'),
          findsOneWidget,
        );
        // the row that could be read is still there, so one mistake does not hide the rest
        expect(find.text('Profit: 1000.00'), findsOneWidget);
      },
    );

    testWidgets(
      'a window can be chosen, and the choice is asked of the engine',
      (tester) async {
        final asked = <Uri>[];
        await _open(tester, _serviceReturning(_october, asked: asked));
        await tester.tap(find.text('That quarter'));
        await tester.pumpAndSettle();
        expect(asked.last.queryParameters['from'], '2026-10-01');
        expect(asked.last.queryParameters['to'], '2026-10-20');
      },
    );

    testWidgets('a week is asked for from its Monday, not from the day', (
      tester,
    ) async {
      final asked = <Uri>[];
      await _open(tester, _serviceReturning(_october, asked: asked));

      await tester.tap(find.text('That week'));
      await tester.pumpAndSettle();

      // 20 October 2026 is a Tuesday, so the week opens on Monday the 19th
      expect(asked.last.queryParameters['from'], '2026-10-19');
      expect(asked.last.queryParameters['to'], '2026-10-20');
    });

    testWidgets('a single day is a window the engine accepts', (tester) async {
      final asked = <Uri>[];
      await _open(tester, _serviceReturning(_october, asked: asked));

      await tester.tap(find.text('That day'));
      await tester.pumpAndSettle();

      expect(asked.last.queryParameters['from'], '2026-10-20');
      expect(asked.last.queryParameters['to'], '2026-10-20');
    });

    testWidgets('the day to report on can be any day, not only today', (
      tester,
    ) async {
      final asked = <Uri>[];
      await _open(tester, _serviceReturning(_october, asked: asked));

      await tester.tap(find.text('20 October 2026'));
      await tester.pumpAndSettle();
      // the picker opens on the day being read; taking it confirms that day rather than jumping elsewhere
      await tester.tap(find.text('OK'));
      await tester.pumpAndSettle();

      expect(asked.last.queryParameters['to'], '2026-10-20');
    });

    testWidgets('a read that failed says so and offers another go', (
      tester,
    ) async {
      await _open(
        tester,
        _serviceReturning(
          '{"detail":"The books are not answering."}',
          status: 503,
        ),
      );
      expect(find.textContaining('could not be read'), findsOneWidget);
      expect(
        find.textContaining('The books are not answering.'),
        findsOneWidget,
      );
      expect(find.text('Try again'), findsOneWidget);
    });
  });
}
