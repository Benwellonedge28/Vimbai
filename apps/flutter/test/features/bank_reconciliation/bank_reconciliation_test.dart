// A bank statement matched against the cash book, on the phone.
//
// Three things are pinned here. The reading, because the difference between the two balances has to be followed to the
// lines that explain it rather than accepted as a verdict. The refusal of a statement that cannot be matched at all -
// no account, no balance, no lines - before anything is sent. And the engine's own words when it refuses, so a person
// reads a sentence about their own statement rather than a status code.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/bank_reconciliation/data/bank_reconciliation_service.dart';
import 'package:vimbai_mobile_client/features/bank_reconciliation/presentation/bank_reconciliation_page.dart';

const _agreed = '''
{"bank_account":"1020","account_name":"Current account","statement_balance":"1287.500000",
 "book_balance":"1225.000000","opening_balance":"1000.000000","difference":"62.500000",
 "statement_lines":3,"posted_entries":3,
 "matched":[
   {"statement_line":1,"statement_date":"2026-10-05","statement_reference":"INV-1",
    "statement_description":"receipt","side":"in","amount":"500.000000","entry_id":"e1",
    "entry_date":"2026-10-05","entry_reference":"INV-1","entry_description":"Sale INV-1"},
   {"statement_line":2,"statement_date":"2026-10-09","statement_reference":"BILL-1",
    "statement_description":"payment","side":"out","amount":"200.000000","entry_id":"e2",
    "entry_date":"2026-10-09","entry_reference":"BILL-1","entry_description":"Bill BILL-1"}],
 "in_statement_only":[
   {"statement_line":3,"date":"2026-10-31","reference":"","description":"monthly charge",
    "side":"out","amount":"12.500000","signed":"-12.500000"}],
 "in_books_only":[
   {"entry_id":"e3","entry_date":"2026-10-28","reference":"CHQ-9","description":"Cheque CHQ-9",
    "signed":"-75.000000","amount":"75.000000","side":"out"}],
 "matched_net":"300.000000","statement_only_net":"-12.500000","book_only_net":"-75.000000",
 "explained_difference":"62.500000","unexplained_difference":"0.000000","agrees":true,
 "summary":"every figure on the statement is either in the books or named here: 2 matched, 1 on the statement alone and 1 in the books alone"}
''';

const _gap = '''
{"bank_account":"1020","account_name":"Current account","statement_balance":"1287.500000",
 "book_balance":"1225.000000","opening_balance":"1000.000000","difference":"62.500000",
 "statement_lines":3,"posted_entries":3,"matched":[],"in_statement_only":[],"in_books_only":[],
 "matched_net":"0.000000","statement_only_net":"0.000000","book_only_net":"0.000000",
 "explained_difference":"0.000000","unexplained_difference":"62.500000","agrees":false,
 "summary":"62.500000 of the difference is not explained by any line: the statement and the books do not agree"}
''';

BankReconciliationService _serviceReturning(
  String body, {
  int status = 200,
  List<Map<String, dynamic>>? sent,
}) {
  final service = BankReconciliationService.fresh();
  service.useClient(
    MockClient((request) async {
      sent?.add({'url': request.url.toString(), 'body': request.body});
      return http.Response(body, status, headers: const {});
    }),
  );
  service.useToken('token');
  return service;
}

Future<void> _show(
  WidgetTester tester,
  BankReconciliationService service,
) async {
  await tester.pumpWidget(
    MaterialApp(home: BankReconciliationPage(service: service)),
  );
  await tester.pumpAndSettle();
}

Future<void> _typeAStatement(WidgetTester tester) async {
  await tester.enterText(find.byType(TextField).at(0), '1020');
  await tester.enterText(find.byType(TextField).at(1), '1287.50');
  await tester.tap(find.text('Add a line'));
  await tester.pumpAndSettle();
  // the form's own fields are still in the tree behind the sheet, so the sheet's fields are named as its own
  Finder inSheet(int index) => find
      .descendant(
        of: find.byType(BottomSheet),
        matching: find.byType(TextField),
      )
      .at(index);
  await tester.enterText(inSheet(0), '2026-10-05');
  await tester.enterText(inSheet(1), '500.00');
  await tester.enterText(inSheet(2), 'INV-1');
  await tester.tap(find.text('Add this line'));
  await tester.pumpAndSettle();
  // the sheet's route leaves the tree over a frame, and a tap that lands on it would be swallowed
  await tester.pump(const Duration(seconds: 1));
}

/// Press the button that asks the engine, once the form is scrolled to it.
Future<void> _matchIt(WidgetTester tester) async {
  final button = find.widgetWithText(FilledButton, 'Match it');
  await tester.ensureVisible(button);
  await tester.pumpAndSettle();
  await tester.tap(button, warnIfMissed: false);
  await tester.pumpAndSettle();
}

void main() {
  group('what the engine answered', () {
    test('a matched line names the entry it is the same movement as', () {
      final answer = Reconciliation.fromJson(
        jsonDecode(_agreed) as Map<String, dynamic>,
      );
      expect(answer.agrees, isTrue);
      expect(answer.unexplainedDifference, '0.000000');
      expect(answer.matched, hasLength(2));
      expect(answer.matched.first.entryReference, 'INV-1');
      expect(answer.matched.first.isIn, isTrue);
      expect(answer.onStatementOnly.single.description, 'monthly charge');
      expect(answer.inBooksOnly.single.reference, 'CHQ-9');
    });

    test('a gap is read as a gap, not as agreement', () {
      final answer = Reconciliation.fromJson(
        jsonDecode(_gap) as Map<String, dynamic>,
      );
      expect(answer.agrees, isFalse);
      expect(answer.difference, '62.500000');
      expect(answer.unexplainedDifference, '62.500000');
    });
  });

  group('the screen', () {
    testWidgets('a statement is typed, sent as given, and read as a proof', (
      tester,
    ) async {
      final sent = <Map<String, dynamic>>[];
      await _show(tester, _serviceReturning(_agreed, sent: sent));
      await _typeAStatement(tester);
      expect(find.textContaining('500.00 in'), findsOneWidget);

      await _matchIt(tester);

      // what was sent is the account, the balance the statement ends on, and the lines as the bank gave them
      final body =
          jsonDecode(sent.single['body'] as String) as Map<String, dynamic>;
      expect(body['bank_account'], '1020');
      expect(body['statement_balance'], '1287.50');
      expect((body['lines'] as List).single, {
        'date': '2026-10-05',
        'amount': '500.00',
        'side': 'in',
        'reference': 'INV-1',
      });

      // and the answer is the proof: both balances, the difference, and nothing left over
      expect(
        find.textContaining('every figure on the statement'),
        findsOneWidget,
      );
      expect(find.text('The statement says'), findsOneWidget);
      expect(find.text('1287.50'), findsWidgets);
      expect(find.text('1225.00'), findsWidgets);
      expect(find.text('62.50'), findsWidgets);
      expect(find.text('Left unexplained'), findsOneWidget);
      expect(find.textContaining('Matched, 2'), findsOneWidget);
      // the two lists of lines one side holds alone are further down the page, so the page is scrolled to them
      await tester.drag(find.byType(ListView), const Offset(0, -500));
      await tester.pumpAndSettle();
      expect(find.textContaining('On the statement only, 1'), findsOneWidget);
      expect(find.textContaining('In your books only, 1'), findsOneWidget);
      expect(find.textContaining('monthly charge'), findsOneWidget);
      expect(find.textContaining('CHQ-9'), findsOneWidget);
    });

    testWidgets('a gap is shown as what is left unexplained', (tester) async {
      await _show(tester, _serviceReturning(_gap));
      await _typeAStatement(tester);
      await _matchIt(tester);
      expect(find.textContaining('do not agree'), findsOneWidget);
      // the figure that is left over is the one the person has to chase
      expect(find.text('62.50'), findsWidgets);
    });

    testWidgets(
      'a statement with no lines is refused before anything is sent',
      (tester) async {
        final sent = <Map<String, dynamic>>[];
        await _show(tester, _serviceReturning(_agreed, sent: sent));
        await tester.enterText(find.byType(TextField).at(0), '1020');
        await tester.enterText(find.byType(TextField).at(1), '1287.50');
        await tester.tap(find.text('Match it'));
        await tester.pumpAndSettle();
        expect(find.textContaining('nothing to match'), findsOneWidget);
        expect(sent, isEmpty);
      },
    );

    testWidgets('a balance that cannot be read is refused in words', (
      tester,
    ) async {
      final sent = <Map<String, dynamic>>[];
      await _show(tester, _serviceReturning(_agreed, sent: sent));
      await tester.enterText(find.byType(TextField).at(0), '1020');
      await tester.enterText(find.byType(TextField).at(1), 'about a thousand');
      await _matchIt(tester);
      expect(
        find.textContaining('not a balance the books can carry'),
        findsOneWidget,
      );
      expect(sent, isEmpty);
    });

    testWidgets('a refusal is shown in the engine\'s own words', (
      tester,
    ) async {
      await _show(
        tester,
        _serviceReturning(
          '{"error":"line 2 carries no side: say in or out"}',
          status: 422,
        ),
      );
      await _typeAStatement(tester);
      await _matchIt(tester);
      expect(find.textContaining('line 2 carries no side'), findsOneWidget);
    });
  });
}
