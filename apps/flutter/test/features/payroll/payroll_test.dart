// Payroll, held to the engine and to the Book's own chart.
//
// Three things are checked here, and they are the three ways this screen could mislead somebody. The figures it shows must
// be the engine's, not a sum added up on the phone. A run that does not balance must not be offered for posting. And the
// accounts a run needs must come from the chart the Book actually has, including when it has none of them yet.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/payroll/data/payroll_posting.dart';
import 'package:vimbai_mobile_client/features/payroll/data/payroll_service.dart';
import 'package:vimbai_mobile_client/features/payroll/presentation/payroll_page.dart';

/// One person's answer, as the engine gives it: 10,000 gross, 6,200 home.
const String _onePerson =
    '{"company_id":"book-1","period":"2026-11","employee_count":1,'
    '"total_gross":"10000.00","total_net":"6200.00","total_paye":"2500.00","total_nassa":"300.00",'
    '"total_pension":"500.00","total_medical":"300.00","total_other_deductions":"200.00",'
    '"employer_pension":"700.00","employer_nassa":"300.00","total_cost_to_company":"11000.00",'
    '"journal_entries":[{"account":"Salaries Expense","debit":"10000.00","credit":"0"},'
    '{"account":"Pension Expense","debit":"700.00","credit":"0"},'
    '{"account":"NSSA Expense","debit":"300.00","credit":"0"},'
    '{"account":"PAYE Payable","debit":"0","credit":"2500.00"},'
    '{"account":"NSSA Payable","debit":"0","credit":"600.00"},'
    '{"account":"Pension Payable","debit":"0","credit":"1200.00"},'
    '{"account":"Medical Aid Payable","debit":"0","credit":"300.00"},'
    '{"account":"Other Deductions Payable","debit":"0","credit":"200.00"},'
    '{"account":"Salaries Payable","debit":"0","credit":"6200.00"}]}';

/// The same shape for two people, with the totals the engine would give for both together.
const String _twoPeople =
    '{"company_id":"book-1","period":"2026-11","employee_count":2,'
    '"total_gross":"15000.00","total_net":"9550.00","total_paye":"3750.00","total_nassa":"450.00",'
    '"total_pension":"750.00","total_medical":"300.00","total_other_deductions":"200.00",'
    '"employer_pension":"1050.00","employer_nassa":"450.00","total_cost_to_company":"16500.00",'
    '"journal_entries":[{"account":"Salaries Expense","debit":"15000.00","credit":"0"},'
    '{"account":"Pension Expense","debit":"1050.00","credit":"0"},'
    '{"account":"NSSA Expense","debit":"450.00","credit":"0"},'
    '{"account":"PAYE Payable","debit":"0","credit":"3750.00"},'
    '{"account":"NSSA Payable","debit":"0","credit":"900.00"},'
    '{"account":"Pension Payable","debit":"0","credit":"1800.00"},'
    '{"account":"Medical Aid Payable","debit":"0","credit":"300.00"},'
    '{"account":"Other Deductions Payable","debit":"0","credit":"200.00"},'
    '{"account":"Salaries Payable","debit":"0","credit":"9550.00"}]}';

/// The shape the engine really answers for somebody with no medical aid and no other deductions: the components are
/// still listed, and the two that took nothing come back as nothing on both sides.
const String _noMedical =
    '{"company_id":"book-1","period":"2026-11","employee_count":1,'
    '"total_gross":"10000.00","total_net":"6700.00","total_paye":"2500.00","total_nassa":"300.00",'
    '"total_pension":"500.00","total_medical":"0","total_other_deductions":"0",'
    '"employer_pension":"700.00","employer_nassa":"300.00","total_cost_to_company":"11000.00",'
    '"journal_entries":[{"account":"Salaries Expense","debit":"10000.00","credit":"0"},'
    '{"account":"Pension Expense","debit":"700.00","credit":"0"},'
    '{"account":"NSSA Expense","debit":"300.00","credit":"0"},'
    '{"account":"PAYE Payable","debit":"0","credit":"2500.00"},'
    '{"account":"NSSA Payable","debit":"0","credit":"600.00"},'
    '{"account":"Pension Payable","debit":"0","credit":"1200.00"},'
    '{"account":"Medical Aid Payable","debit":"0","credit":"0"},'
    '{"account":"Other Deductions Payable","debit":"0","credit":"0"},'
    '{"account":"Salaries Payable","debit":"0","credit":"6700.00"}]}';

/// A service answering from canned engine answers, and every request it was sent.
class _Fake {
  _Fake({String? onePerson, String? twoPeople})
    : _one = onePerson ?? _onePerson,
      _two = twoPeople ?? _twoPeople;

  final String _one;
  final String _two;

  /// Every request the page and the service actually sent.
  final List<http.Request> sent = [];

  late final PayrollService service = PayrollService.fresh()
    ..useClient(
      MockClient((request) async {
        sent.add(request);
        final body = jsonDecode(request.body) as Map<String, dynamic>;
        final employees = (body['employees'] as List).length;
        return http.Response(employees > 1 ? _two : _one, 200);
      }),
    );
}

Account _account(String number, String name, String type, String normal) =>
    Account(
      accountNumber: number,
      accountName: name,
      accountType: type,
      normalBalance: normal,
    );

void main() {
  test('a run asks for the whole payroll, then for each person', () async {
    final fake = _Fake();
    final run = await fake.service.run(
      period: '2026-11',
      people: [
        PayrollPerson(name: 'Tendai', gross: ExactAmount.parse('10000')),
        PayrollPerson(
          name: 'Rudo',
          gross: ExactAmount.parse('5000'),
          medicalAid: ExactAmount.parse('0'),
        ),
      ],
    );

    // one call for the payroll, one for each person - so a per-person figure is still the engine's
    expect(fake.sent.length, 3);
    expect((fake.sent.first.body).contains('"employee_name":"Tendai"'), isTrue);
    expect(run.people, 2);
    expect(run.costToCompany.toDecimalString(), '16500.00');
    expect(run.net.toDecimalString(), '9550.00');
    expect(run.payslips.length, 2);
    expect(run.payslips.first.name, 'Tendai');
    expect(run.payslips.first.net.toDecimalString(), '6200.00');
    expect(run.payslips.last.name, 'Rudo');
  });

  test(
    'the cost to the business is the engine total, contributions included',
    () async {
      final run = await _Fake().service.run(
        period: '2026-11',
        people: [
          PayrollPerson(name: 'Tendai', gross: ExactAmount.parse('10000')),
        ],
      );

      expect(run.employerContributions.toDecimalString(), '1000.00');
      expect(run.costToCompany.toDecimalString(), '11000.00');
      // the run posts nine lines and they are the engine's own, adding up on both sides
      expect(run.lines.length, 9);
      expect(run.balanced, isTrue);
    },
  );

  test(
    'a payroll with nobody on it is refused before anything is sent',
    () async {
      final fake = _Fake();
      await expectLater(
        fake.service.run(period: '2026-11', people: const []),
        throwsA(
          predicate((failure) => '$failure'.contains('at least one person')),
        ),
      );
      expect(fake.sent, isEmpty);
    },
  );

  test(
    'a row with no name or no pay is left out, not sent as a paid nobody',
    () async {
      final fake = _Fake();
      await fake.service.run(
        period: '2026-11',
        people: [
          PayrollPerson(name: 'Tendai', gross: ExactAmount.parse('10000')),
          PayrollPerson(name: '', gross: ExactAmount.parse('4000')),
          PayrollPerson(name: 'Rudo', gross: ExactAmount.zero),
        ],
      );

      final sent = jsonDecode(fake.sent.first.body) as Map<String, dynamic>;
      expect((sent['employees'] as List).length, 1);
    },
  );

  test(
    'a month left blank is refused, because a payroll belongs to one',
    () async {
      await expectLater(
        _Fake().service.run(
          period: '  ',
          people: [
            PayrollPerson(name: 'Tendai', gross: ExactAmount.parse('10000')),
          ],
        ),
        throwsA(predicate((failure) => '$failure'.contains('which month'))),
      );
    },
  );

  test(
    'a run whose lines do not add up says so instead of offering to post',
    () async {
      final broken = _onePerson.replaceFirst(
        '"credit":"6200.00"',
        '"credit":"6000.00"',
      );
      final run = await _Fake(onePerson: broken).service.run(
        period: '2026-11',
        people: [
          PayrollPerson(name: 'Tendai', gross: ExactAmount.parse('10000')),
        ],
      );

      expect(run.balanced, isFalse);
    },
  );

  test(
    'lines that move nothing are left out, and ask for no account of their own',
    () async {
      // The ledger refuses such a line by name - "line 6 moves no money" - so a month with no medical aid could never be
      // posted until this was noticed. The calculator names every component whether or not anything went to it.
      final run = await _Fake(onePerson: _noMedical).service.run(
        period: '2026-11',
        people: [
          PayrollPerson(name: 'Tendai', gross: ExactAmount.parse('10000')),
        ],
      );

      expect(run.lines.length, 9);
      expect(run.postableLines.length, 7);
      expect(
        run.postableLines.map((line) => line.account),
        isNot(contains('Medical Aid Payable')),
      );
      expect(run.balanced, isTrue);
      // a chart is only asked for the accounts the entry will use
      final needs = neededAccounts(run.postableLines);
      expect(needs.length, 7);
      expect(
        needs.map((need) => need.name),
        isNot(contains('Other Deductions Payable')),
      );
    },
  );

  test(
    'an account is needed per journal line, typed by the side it sits on',
    () {
      final run = PayrollRun.fromJson(
        jsonDecode(_onePerson) as Map<String, dynamic>,
      );
      final needs = neededAccounts(run.lines);

      expect(needs.length, 9);
      final salaries = needs.firstWhere(
        (need) => need.name == 'Salaries Expense',
      );
      expect(salaries.accountType, 'expense');
      expect(salaries.normalBalance, 'debit');
      final paye = needs.firstWhere((need) => need.name == 'PAYE Payable');
      expect(paye.accountType, 'liability');
      expect(paye.normalBalance, 'credit');
    },
  );

  test(
    'accounts are matched by name, whatever case and spacing the chart uses',
    () {
      final run = PayrollRun.fromJson(
        jsonDecode(_onePerson) as Map<String, dynamic>,
      );
      final needs = neededAccounts(run.lines);
      final chart = [
        _account('6100', '  salaries EXPENSE ', 'expense', 'debit'),
        _account('2100', 'PAYE Payable', 'liability', 'credit'),
      ];

      final matched = matchAccounts(needs, chart);
      expect(matched['Salaries Expense']!.accountNumber, '6100');
      expect(matched['PAYE Payable']!.accountNumber, '2100');
      // everything the chart does not have is named as missing, and the rest are not
      final missing = missingAccounts(needs, matched);
      expect(missing.length, 7);
      expect(missing.map((need) => need.name), contains('Salaries Payable'));
      expect(missing.map((need) => need.name), isNot(contains('PAYE Payable')));
    },
  );

  test('a new account gets a number the chart has not already spent', () {
    expect(freeAccountNumber('6100', {}), '6100');
    expect(freeAccountNumber('6100', {'6100'}), '6110');
    expect(freeAccountNumber('2100', {'2100', '2110', '2120'}), '2130');
  });

  testWidgets(
    'the page leads with what the month costs and what each person gets',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          home: PayrollPage(
            service: _Fake().service,
            clock: DateTime(2026, 11, 18),
          ),
        ),
      );

      await tester.enterText(find.widgetWithText(TextField, 'Name'), 'Tendai');
      await tester.enterText(
        find.widgetWithText(TextField, 'Gross pay'),
        '10000',
      );
      await tester.tap(find.text('Work out the run'));
      await tester.pumpAndSettle();

      expect(find.text('This payroll costs 11000.00'), findsOneWidget);
      // once in the name field the person typed, once on the payslip below
      expect(find.text('Tendai'), findsNWidgets(2));
      expect(
        find.text('Gross 10000.00, 3800.00 off, takes home 6200.00'),
        findsOneWidget,
      );
      expect(find.text('Post it to the ledger'), findsOneWidget);
    },
  );

  testWidgets('a run that does not add up cannot be posted from the page', (
    tester,
  ) async {
    final broken = _onePerson.replaceFirst(
      '"credit":"6200.00"',
      '"credit":"6000.00"',
    );
    await tester.pumpWidget(
      MaterialApp(
        home: PayrollPage(
          service: _Fake(onePerson: broken).service,
          clock: DateTime(2026, 11, 18),
        ),
      ),
    );

    await tester.enterText(find.widgetWithText(TextField, 'Name'), 'Tendai');
    await tester.enterText(
      find.widgetWithText(TextField, 'Gross pay'),
      '10000',
    );
    await tester.tap(find.text('Work out the run'));
    await tester.pumpAndSettle();

    expect(
      find.text('This entry does not add up, so it will not be posted.'),
      findsOneWidget,
    );
    final button = tester.widget<ElevatedButton>(
      find.widgetWithText(ElevatedButton, 'Post it to the ledger'),
    );
    expect(button.onPressed, isNull);
  });

  testWidgets(
    'a payroll with nobody on it explains what to do instead of posting nothing',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          home: PayrollPage(
            service: _Fake().service,
            clock: DateTime(2026, 11, 18),
          ),
        ),
      );

      await tester.tap(find.text('Work out the run'));
      await tester.pumpAndSettle();

      expect(
        find.text(
          'Add at least one person and what they are paid, then work out the run.',
        ),
        findsOneWidget,
      );
    },
  );
}
