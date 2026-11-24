// Payroll, held to the engine and to the Book's own chart.
//
// Three things are checked here, and they are the three ways this screen could mislead somebody. The figures it shows must
// be the engine's, not a sum added up on the phone. A run that does not balance must not be offered for posting. And what
// the posting carries must be the run's own totals to the engine's payroll journal, so the accounts and the dating are the
// engine's decisions rather than a journal entry built on the phone.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
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
  _Fake({String? onePerson, String? twoPeople, String? posted})
    : _one = onePerson ?? _onePerson,
      _two = twoPeople ?? _twoPeople,
      _posted = posted ?? _postedRun;

  final String _one;
  final String _two;

  /// What the payroll journal answers when it posts, or a refusal when a test wants one.
  final String _posted;

  /// Every request the page and the service actually sent.
  final List<http.Request> sent = [];

  late final PayrollService service = PayrollService.fresh()
    ..useClient(
      MockClient((request) async {
        sent.add(request);
        // the journal's own door, which is not the calculator's and carries no people
        if (request.url.path.endsWith('/accounting/payroll')) {
          return http.Response(
            _posted,
            _posted.contains('"detail"') ? 422 : 201,
          );
        }
        final body = jsonDecode(request.body) as Map<String, dynamic>;
        final employees = (body['employees'] as List).length;
        return http.Response(employees > 1 ? _two : _one, 200);
      }),
    );

  /// What the journal was told, which is the run's totals and nothing else.
  Map<String, dynamic> get journalRequest =>
      jsonDecode(
            sent
                .lastWhere(
                  (request) => request.url.path.endsWith('/accounting/payroll'),
                )
                .body,
          )
          as Map<String, dynamic>;
}

/// What the engine's payroll journal answers when a month of wages is posted.
const String _postedRun =
    '{"period":"2026-11","reference_number":"PAY 2026-11","gross_wages":"10000.000000",'
    '"withheld":"3500.000000","employer_contributions":"1000.000000","net_pay":"6500.000000",'
    '"total_cost":"11000.000000","wages_account":"6250","deductions_account":"2260",'
    '"net_account":"2250","journal_entry_id":"entry-1","entry_date":"2026-11-30"}';

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
    'the posting carries the run\'s own totals to the engine\'s journal',
    () async {
      final fake = _Fake();
      final run = await fake.service.run(
        period: '2026-11',
        people: [
          PayrollPerson(name: 'Tendai', gross: ExactAmount.parse('10000')),
        ],
      );
      final posted = await fake.service.postRun(run);

      final sent = fake.journalRequest;
      // what the work cost, what came off the payslips and what the employer adds: stated, never a rate applied here
      expect(sent['period'], '2026-11');
      expect(sent['reference'], 'PAY 2026-11');
      expect(sent['gross'], '10000.00');
      expect(sent['paye'], '2500.00');
      expect(sent['nassa'], '300.00');
      expect(sent['pension'], '500.00');
      expect(sent['medical_aid'], '300.00');
      expect(sent['other_deductions'], '200.00');
      expect(sent['employer_contributions'], '1000.00');
      // no journal entry is built on the phone: the accounts and the dating are the engine's
      expect(sent.containsKey('lines'), isFalse);
      expect(sent.containsKey('entry_date'), isFalse);
      expect(sent.containsKey('employees'), isFalse);

      // and the figures shown afterwards are the engine's own answer
      expect(posted.netPay, '6500.000000');
      expect(posted.totalCost, '11000.000000');
      expect(posted.entryDate, '2026-11-30');
      expect(posted.deductionsAccount, '2260');
    },
  );

  test('a component that was nothing is not sent at all', () async {
    final fake = _Fake(onePerson: _noMedical);
    final run = await fake.service.run(
      period: '2026-11',
      people: [
        PayrollPerson(name: 'Tendai', gross: ExactAmount.parse('10000')),
      ],
    );
    await fake.service.postRun(run);

    final sent = fake.journalRequest;
    // an empty box is a thing the business does not have, not a figure of nothing to record
    expect(sent.containsKey('medical_aid'), isFalse);
    expect(sent.containsKey('other_deductions'), isFalse);
    expect(sent['paye'], '2500.00');
  });

  test('the engine\'s refusal is repeated in its own words', () async {
    final fake = _Fake(
      posted:
          '{"detail":"Journal Entry with reference PAY 2026-11 already exists for source Payroll."}',
    );
    final run = await fake.service.run(
      period: '2026-11',
      people: [
        PayrollPerson(name: 'Tendai', gross: ExactAmount.parse('10000')),
      ],
    );

    await expectLater(
      fake.service.postRun(run),
      throwsA(
        predicate(
          (failure) => '$failure'.contains('already exists for source Payroll'),
        ),
      ),
    );
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
      expect(find.text('Post it to the books'), findsOneWidget);

      // posting asks first, then says what the books now say, in the engine's figures
      final post = find.widgetWithText(ElevatedButton, 'Post it to the books');
      await tester.ensureVisible(post);
      await tester.pumpAndSettle();
      await tester.tap(post);
      await tester.pumpAndSettle();
      expect(find.text('Post the wages for 2026-11'), findsOneWidget);
      await tester.tap(find.text('Post it'));
      await tester.pumpAndSettle();

      expect(find.text('The books now say'), findsOneWidget);
      expect(find.text('In the books'), findsOneWidget);
      expect(find.textContaining('6500.000000 owed to staff'), findsOneWidget);
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
      find.widgetWithText(ElevatedButton, 'Post it to the books'),
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
