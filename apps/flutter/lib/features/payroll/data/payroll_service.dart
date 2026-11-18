// Running a payroll, and putting it into the books.
//
// The arithmetic is not here. The engine's payroll calculator takes a list of people and their pay and answers with the
// totals a business has to find - gross, PAYE, NSSA, pension, medical, other deductions, net pay, cost to company - and
// with the journal the run posts. This service carries the people there and the answer back, so the phone never adds up
// a payroll of its own and cannot disagree with the books it came from.
//
// A run is one call for the whole payroll (that is the figure the business pays) and one call per person (that is what
// each person takes home). Two calls, not one sum done here: a total the engine rounded and a total this app added from
// its parts can differ by a cent, and a payroll a cent out is a payroll somebody has to redo.
//
// Amounts travel as exact decimal text and are printed through ExactAmount, for the same reason as everywhere else: a
// salary that passed through a floating point number would not be the salary.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One person on the payroll, as the person running it enters them.
class PayrollPerson {
  final String name;
  final ExactAmount gross;
  final ExactAmount medicalAid;
  final ExactAmount otherDeductions;

  const PayrollPerson({
    required this.name,
    required this.gross,
    this.medicalAid = ExactAmount.zero,
    this.otherDeductions = ExactAmount.zero,
  });

  /// Whether this person is worth sending: a row somebody opened and never filled in is not a person.
  bool get isFilled => name.trim().isNotEmpty && !gross.isZero;
}

/// What one person takes home, and what came off their pay to get there.
class Payslip {
  final String name;
  final ExactAmount gross;
  final ExactAmount paye;
  final ExactAmount nassa;
  final ExactAmount pension;
  final ExactAmount medical;
  final ExactAmount other;
  final ExactAmount net;

  const Payslip({
    required this.name,
    required this.gross,
    required this.paye,
    required this.nassa,
    required this.pension,
    required this.medical,
    required this.other,
    required this.net,
  });

  ExactAmount get deductions => paye + nassa + pension + medical + other;

  factory Payslip.fromJson(String name, Map<String, dynamic> json) => Payslip(
    name: name,
    gross: ExactAmount.fromJson(json['total_gross']),
    paye: ExactAmount.fromJson(json['total_paye']),
    nassa: ExactAmount.fromJson(json['total_nassa']),
    pension: ExactAmount.fromJson(json['total_pension']),
    medical: ExactAmount.fromJson(json['total_medical']),
    other: ExactAmount.fromJson(json['total_other_deductions']),
    net: ExactAmount.fromJson(json['total_net']),
  );
}

/// One side of the journal the run posts.
class PayrollLine {
  final String account;
  final ExactAmount debit;
  final ExactAmount credit;

  const PayrollLine({
    required this.account,
    required this.debit,
    required this.credit,
  });

  factory PayrollLine.fromJson(Map<String, dynamic> json) => PayrollLine(
    account: (json['account'] as String?) ?? '',
    debit: ExactAmount.fromJson(json['debit']),
    credit: ExactAmount.fromJson(json['credit']),
  );
}

/// A payroll run for one period: what it costs, what each person gets, and the journal it posts.
class PayrollRun {
  final String period;
  final int people;
  final ExactAmount gross;
  final ExactAmount paye;
  final ExactAmount nassa;
  final ExactAmount pension;
  final ExactAmount medical;
  final ExactAmount other;
  final ExactAmount net;
  final ExactAmount employerPension;
  final ExactAmount employerNassa;
  final ExactAmount costToCompany;
  final List<PayrollLine> lines;
  final List<Payslip> payslips;

  const PayrollRun({
    required this.period,
    required this.people,
    required this.gross,
    required this.paye,
    required this.nassa,
    required this.pension,
    required this.medical,
    required this.other,
    required this.net,
    required this.employerPension,
    required this.employerNassa,
    required this.costToCompany,
    required this.lines,
    required this.payslips,
  });

  /// The lines that actually move money, which are the lines the ledger will take.
  ///
  /// The calculator lists every component of a payroll whether or not anything went to it, so a month with no medical aid
  /// comes back with a Medical Aid Payable line of nothing on both sides. The ledger refuses such a line by name ("line 6
  /// moves no money"), and it is right to: an entry that says somebody is owed nothing is a line to explain later. So the
  /// empties are dropped here, once, rather than each caller remembering to.
  List<PayrollLine> get postableLines => [
    for (final line in lines)
      if (!line.debit.isZero || !line.credit.isZero) line,
  ];

  /// What the run costs beyond the wages: the employer's own contributions.
  ExactAmount get employerContributions => employerPension + employerNassa;

  /// True when the journal the engine proposed adds up on both sides.
  ///
  /// A run that does not balance cannot be posted, and saying so is better than offering a button the ledger will
  /// refuse. This checks the engine's own lines rather than a total built here, so it notices the engine changing.
  bool get balanced {
    final debits = ExactAmount.sum(lines.map((line) => line.debit));
    final credits = ExactAmount.sum(lines.map((line) => line.credit));
    return debits == credits;
  }

  /// The reference the posting carries.
  ///
  /// The engine refuses a second entry with the same reference, so one period's payroll can be posted once and a repeat
  /// is refused by name rather than paying everybody a second time.
  String get reference => 'PAY $period';

  factory PayrollRun.fromJson(
    Map<String, dynamic> json, {
    List<Payslip> payslips = const [],
  }) => PayrollRun(
    period: (json['period'] as String?) ?? '',
    people: (json['employee_count'] as num?)?.toInt() ?? 0,
    gross: ExactAmount.fromJson(json['total_gross']),
    paye: ExactAmount.fromJson(json['total_paye']),
    nassa: ExactAmount.fromJson(json['total_nassa']),
    pension: ExactAmount.fromJson(json['total_pension']),
    medical: ExactAmount.fromJson(json['total_medical']),
    other: ExactAmount.fromJson(json['total_other_deductions']),
    net: ExactAmount.fromJson(json['total_net']),
    employerPension: ExactAmount.fromJson(json['employer_pension']),
    employerNassa: ExactAmount.fromJson(json['employer_nassa']),
    costToCompany: ExactAmount.fromJson(json['total_cost_to_company']),
    lines: [
      for (final line in (json['journal_entries'] as List?) ?? const [])
        PayrollLine.fromJson(line as Map<String, dynamic>),
    ],
    payslips: payslips,
  );
}

/// The payroll door of one Book.
class PayrollService {
  PayrollService._();

  static final PayrollService instance = PayrollService._();

  /// A second instance for a test that needs a transport of its own.
  PayrollService.fresh();

  static const String _base = AppConfig.apiUrl;

  /// The engine's payroll calculator, behind the gateway's `/calc` door.
  static const String _door = '/calc/payroll-accounting-process';

  http.Client _client = http.Client();

  /// A test calls the service against a transport it controls. Nothing in the app sets this.
  @visibleForTesting
  void useClient(http.Client client) => _client = client;

  String? _token;

  /// A test may act as a particular session; the app leaves this alone so the session's own token is used.
  @visibleForTesting
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// Work out a run for [period] (`2026-11`) from the people entered.
  ///
  /// Refused here rather than sent: a run with nobody on it, and a period left blank. A person with no name or no gross
  /// pay is left out - they earn nothing this period, which is a fact about them rather than a payroll line, and a line
  /// of nothing only adds something for the next person to wonder about.
  Future<PayrollRun> run({
    required String period,
    required List<PayrollPerson> people,
  }) async {
    if (period.trim().isEmpty) {
      throw Exception('Say which month this payroll is for, as 2026-11.');
    }
    final paid = people.where((person) => person.isFilled).toList();
    if (paid.isEmpty) {
      throw Exception(
        'Add at least one person and what they are paid, then work out the run.',
      );
    }
    final totals = await _calculate(period, paid);
    final payslips = <Payslip>[];
    for (final person in paid) {
      final one = await _calculate(period, [person]);
      payslips.add(Payslip.fromJson(person.name.trim(), one));
    }
    return PayrollRun.fromJson(totals, payslips: payslips);
  }

  Future<Map<String, dynamic>> _calculate(
    String period,
    List<PayrollPerson> people,
  ) async {
    final answer = await _client.post(
      Uri.parse('$_base$_door'),
      headers: _headers,
      body: jsonEncode({
        // the calculator's published shape names the company; a Book is the company a run belongs to
        'company_id': BookContext.instance.current?.id ?? '',
        'period': period.trim(),
        'employees': [
          for (final person in people)
            {
              'employee_name': person.name.trim(),
              'gross_salary': person.gross.toLedgerString(),
              'medical_aid': person.medicalAid.toLedgerString(),
              'other_deductions': person.otherDeductions.toLedgerString(),
            },
        ],
      }),
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return jsonDecode(answer.body) as Map<String, dynamic>;
  }
}
