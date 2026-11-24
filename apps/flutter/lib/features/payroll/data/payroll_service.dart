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

/// A month's wages, as the engine's payroll journal posted it.
///
/// The figures are the engine's own: the net owed and the total cost are worked out where the entry is posted, from the
/// run's stated amounts, so what the screen shows after posting is what the books now say rather than a second sum.
class PostedPayroll {
  const PostedPayroll({
    required this.period,
    required this.reference,
    required this.grossWages,
    required this.withheld,
    required this.employerContributions,
    required this.netPay,
    required this.totalCost,
    required this.wagesAccount,
    required this.deductionsAccount,
    required this.netAccount,
    required this.journalEntryId,
    required this.entryDate,
  });

  final String period;
  final String reference;
  final String grossWages;

  /// Everything withheld from the payslips, added up by the engine.
  final String withheld;
  final String employerContributions;

  /// What the staff are still owed.
  final String netPay;

  /// What the month cost the business: the gross plus the employer's own contribution.
  final String totalCost;
  final String wagesAccount;
  final String deductionsAccount;
  final String netAccount;
  final String journalEntryId;

  /// The last day of the month the entry is dated in.
  final String entryDate;

  factory PostedPayroll.fromJson(Map<String, dynamic> json) => PostedPayroll(
    period: (json['period'] as String?) ?? '',
    reference: (json['reference_number'] as String?) ?? '',
    grossWages: (json['gross_wages'] as String?) ?? '',
    withheld: (json['withheld'] as String?) ?? '',
    employerContributions: (json['employer_contributions'] as String?) ?? '',
    netPay: (json['net_pay'] as String?) ?? '',
    totalCost: (json['total_cost'] as String?) ?? '',
    wagesAccount: (json['wages_account'] as String?) ?? '',
    deductionsAccount: (json['deductions_account'] as String?) ?? '',
    netAccount: (json['net_account'] as String?) ?? '',
    journalEntryId: (json['journal_entry_id'] as String?) ?? '',
    entryDate: (json['entry_date'] as String?) ?? '',
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

  /// Put the month into the books.
  ///
  /// The run was worked out by the engine's calculator, so its totals are what is sent: the journal posts what the work
  /// cost, what the staff are owed and what is held for the tax authority, the pension fund and everybody else a payslip
  /// withholds for. The accounts are the Book's own, found by name in its chart rather than fixed here, and a chart that
  /// has none of them is refused by the engine in words that say which account to name.
  ///
  /// The run's own reference travels with it, so the same month cannot be posted twice: the engine refuses the second by
  /// name rather than paying everybody again.
  Future<PostedPayroll> postRun(PayrollRun run) async {
    final answer = await _client.post(
      Uri.parse('$_base/accounting/payroll'),
      headers: _headers,
      body: jsonEncode({
        'period': run.period.trim(),
        'reference': run.reference,
        'gross': run.gross.toDecimalString(),
        if (!run.paye.isZero) 'paye': run.paye.toDecimalString(),
        if (!run.nassa.isZero) 'nassa': run.nassa.toDecimalString(),
        if (!run.pension.isZero) 'pension': run.pension.toDecimalString(),
        if (!run.medical.isZero) 'medical_aid': run.medical.toDecimalString(),
        if (!run.other.isZero) 'other_deductions': run.other.toDecimalString(),
        if (!run.employerContributions.isZero)
          'employer_contributions': run.employerContributions.toDecimalString(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(_refusalOf(answer));
    }
    return PostedPayroll.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// The engine's own sentence when it refuses, rather than a status code and a wall of JSON.
  String _refusalOf(http.Response answer) {
    try {
      final body = jsonDecode(answer.body);
      if (body is Map<String, dynamic>) {
        for (final key in ['detail', 'message', 'error']) {
          final words = body[key];
          if (words is String && words.trim().isNotEmpty) return words;
        }
      }
    } catch (_) {
      // the body was not the engine's envelope, so the status is all there is to say
    }
    return 'The engine answered ${answer.statusCode}.';
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
      throw Exception(_refusalOf(answer));
    }
    return jsonDecode(answer.body) as Map<String, dynamic>;
  }
}
