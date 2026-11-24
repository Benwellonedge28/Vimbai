// Payroll: the people, what they are paid, what it costs, and posting the month to the books.
//
// A payroll screen has two jobs and they are not the same job. The first is working out the month: who is paid what, what
// comes off each person's pay, what each of them takes home, and what the whole thing costs the business once its own
// pension and NSSA contributions are counted. The second is putting the month into the ledger, because a business that
// has paid everybody and whose books have never heard of it has a payroll that only exists in the bank.
//
// The arithmetic is the engine's, in both parts: the totals come from its payroll calculator, and the posting is its
// payroll journal, which finds the accounts in the Book's own chart and dates the entry in the month it belongs to.
// Nothing here adds a salary up and nothing here builds a journal entry. That is deliberate - the figure a person is
// shown has to be the figure that will be posted, or the screen is decoration.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/payroll/data/payroll_service.dart';

class PayrollPage extends StatefulWidget {
  const PayrollPage({super.key, this.service, this.clock});

  /// A test passes its own service so the page can be exercised without a server.
  final PayrollService? service;

  /// A test fixes the month, so "this month" does not depend on the day it runs.
  final DateTime? clock;

  @override
  State<PayrollPage> createState() => _PayrollPageState();
}

/// One row of the sheet somebody fills in: a name and what they are paid.
class _PersonRow {
  _PersonRow({String name = ''}) : name = TextEditingController(text: name);

  final TextEditingController name;
  final TextEditingController gross = TextEditingController();
  final TextEditingController medical = TextEditingController();
  final TextEditingController other = TextEditingController();

  bool get isEmpty =>
      name.text.trim().isEmpty &&
      gross.text.trim().isEmpty &&
      medical.text.trim().isEmpty &&
      other.text.trim().isEmpty;

  void dispose() {
    name.dispose();
    gross.dispose();
    medical.dispose();
    other.dispose();
  }
}

class _PayrollPageState extends State<PayrollPage> {
  late final PayrollService _payroll =
      widget.service ?? PayrollService.instance;

  late final TextEditingController _period;
  final List<_PersonRow> _people = [_PersonRow()];

  PayrollRun? _run;
  bool _working = false;
  bool _posting = false;
  bool _posted = false;

  /// What the engine answered when it posted, so the page repeats its figures rather than its own.
  PostedPayroll? _postedRun;
  String? _problem;

  @override
  void initState() {
    super.initState();
    final now = widget.clock ?? DateTime.now();
    _period = TextEditingController(
      text: '${now.year}-${now.month.toString().padLeft(2, '0')}',
    );
  }

  @override
  void dispose() {
    _period.dispose();
    for (final row in _people) {
      row.dispose();
    }
    super.dispose();
  }

  /// An amount typed by a person, counted exactly.
  ///
  /// Spaces and thousands separators are dropped because people write 10,000 and mean ten thousand. Anything left that is
  /// not a number is refused by name: silently treating "10k" as zero would pay somebody nothing.
  ExactAmount _amount(String text, String what) {
    final cleaned = text.replaceAll(',', '').replaceAll(' ', '').trim();
    if (cleaned.isEmpty) return ExactAmount.zero;
    final amount = ExactAmount.tryParse(cleaned);
    if (amount == null) {
      throw Exception('$what is not an amount: "${text.trim()}".');
    }
    if (amount.isNegative) {
      throw Exception('$what cannot be less than nothing.');
    }
    return amount;
  }

  Future<void> _workOutTheRun() async {
    if (_working) return;
    setState(() {
      _working = true;
      _problem = null;
      _run = null;
      _posted = false;
    });
    try {
      final people = <PayrollPerson>[];
      for (final row in _people) {
        if (row.isEmpty) continue;
        final who = row.name.text.trim();
        if (who.isEmpty) {
          throw Exception('One of the people on this run has no name.');
        }
        people.add(
          PayrollPerson(
            name: who,
            gross: _amount(row.gross.text, '$who\'s gross pay'),
            medicalAid: _amount(row.medical.text, '$who\'s medical aid'),
            otherDeductions: _amount(
              row.other.text,
              '$who\'s other deductions',
            ),
          ),
        );
      }
      final run = await _payroll.run(period: _period.text, people: people);
      if (!mounted) return;
      setState(() {
        _run = run;
        _working = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _working = false;
        _problem = CouldNotRead.reasonOf(failure);
      });
    }
  }

  /// Put the month into the books.
  ///
  /// The run's own figures are what is sent. The engine's payroll journal posts what the work cost, what the staff are
  /// owed and what the business is holding for the tax authority, the pension fund and everybody else a payslip withholds
  /// for, dated the last day of the month it belongs to. It finds the accounts by name in this Book's own chart, so a
  /// business that calls its wage account "Wages and Salaries" still posts its payroll, and a chart with none of them is
  /// refused in words that say which account to name rather than by a status code.
  ///
  /// Nothing here moves money: the wages that were actually paid belong in the cash book, and this says what the month
  /// cost and what it owes.
  Future<void> _postTheRun() async {
    final run = _run;
    if (run == null || _posting || _posted) return;
    setState(() {
      _posting = true;
      _problem = null;
    });
    try {
      if (!run.balanced) {
        throw Exception(
          'The run the engine worked out does not add up to the same on both sides, so it was not posted.',
        );
      }
      final confirmed = await _showProposal(run);
      if (confirmed != true || !mounted) {
        setState(() => _posting = false);
        return;
      }
      final posted = await _payroll.postRun(run);
      if (!mounted) return;
      setState(() {
        _posting = false;
        _posted = true;
        _postedRun = posted;
      });
      _say('The payroll for ${posted.period} is in the books.');
    } catch (failure) {
      if (!mounted) return;
      setState(() => _posting = false);
      _say(CouldNotRead.reasonOf(failure));
    }
  }

  /// What the month will say, in the terms of the business rather than the ledger, before it is written.
  Future<bool?> _showProposal(PayrollRun run) => showDialog<bool>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: Text('Post the wages for ${run.period}'),
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'The work cost ${run.costToCompany.toDecimalString()}: '
              '${run.gross.toDecimalString()} of wages and '
              '${run.employerContributions.toDecimalString()} the business adds on top.',
            ),
            const SizedBox(height: 8),
            Text(
              '${run.net.toDecimalString()} is owed to the staff, and '
              '${(run.paye + run.nassa + run.pension + run.medical + run.other + run.employerContributions).toDecimalString()} '
              'is held for the tax authority, the pension fund and anybody else the payslips withhold for.',
            ),
            const SizedBox(height: 8),
            Text(
              'No money moves. Reference ${run.reference}; a posted entry is corrected by reversing it, never edited.',
              style: Theme.of(ctx).textTheme.bodySmall,
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(ctx).pop(false),
          child: const Text('Not now'),
        ),
        ElevatedButton(
          onPressed: () => Navigator.of(ctx).pop(true),
          child: const Text('Post it'),
        ),
      ],
    ),
  );

  void _say(String words) => ScaffoldMessenger.of(
    context,
  ).showSnackBar(SnackBar(content: Text(words)));

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Payroll')),
      // A form and the answer it produced, built eagerly: the whole month's figures belong to one page, and a list that
      // throws away the part below the fold would lose the answer the moment the person scrolls back up to fix a name.
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              controller: _period,
              decoration: const InputDecoration(
                labelText: 'Month',
                helperText: 'The payroll period, as 2026-11',
              ),
            ),
            const SizedBox(height: 16),
            Text('Who is paid', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 4),
            Text(
              'Gross pay, and anything taken off it beyond tax: medical aid and other deductions.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            for (final row in _people) _personFields(row),
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton.icon(
                onPressed: () => setState(() => _people.add(_PersonRow())),
                icon: const Icon(Icons.person_add_alt),
                label: const Text('Another person'),
              ),
            ),
            const SizedBox(height: 8),
            ElevatedButton(
              onPressed: _working ? null : _workOutTheRun,
              child: Text(_working ? 'Working it out' : 'Work out the run'),
            ),
            if (_problem != null) ...[
              const SizedBox(height: 12),
              Text(_problem!, style: Theme.of(context).textTheme.bodyMedium),
            ],
            if (_run != null) ...[const SizedBox(height: 20), _answer(_run!)],
          ],
        ),
      ),
    );
  }

  Widget _personFields(_PersonRow row) => Card(
    margin: const EdgeInsets.only(top: 12),
    child: Padding(
      padding: const EdgeInsets.all(12),
      child: Column(
        children: [
          TextField(
            controller: row.name,
            decoration: const InputDecoration(labelText: 'Name'),
          ),
          TextField(
            controller: row.gross,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(labelText: 'Gross pay'),
          ),
          TextField(
            controller: row.medical,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'Medical aid (if any)',
            ),
          ),
          TextField(
            controller: row.other,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'Other deductions (if any)',
            ),
          ),
        ],
      ),
    ),
  );

  Widget _answer(PayrollRun run) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(
        'This payroll costs ${run.costToCompany.toDecimalString()}',
        style: Theme.of(context).textTheme.titleLarge,
      ),
      const SizedBox(height: 4),
      Text(
        '${run.people} ${run.people == 1 ? 'person' : 'people'}, ${run.period}. '
        'Gross ${run.gross.toDecimalString()}, and '
        '${run.employerContributions.toDecimalString()} the business adds on top.',
        style: Theme.of(context).textTheme.bodySmall,
      ),
      const SizedBox(height: 12),
      _figure('Gross pay', run.gross),
      _figure('PAYE', run.paye),
      _figure('NSSA (employee)', run.nassa),
      _figure('Pension (employee)', run.pension),
      _figure('Medical aid', run.medical),
      _figure('Other deductions', run.other),
      _figure('Net pay out', run.net),
      const Divider(height: 24),
      _figure('Pension (employer)', run.employerPension),
      _figure('NSSA (employer)', run.employerNassa),
      _figure('Cost to the business', run.costToCompany),
      if (run.payslips.isNotEmpty) ...[
        const SizedBox(height: 20),
        Text(
          'What each person gets',
          style: Theme.of(context).textTheme.titleMedium,
        ),
        const SizedBox(height: 8),
        for (final slip in run.payslips) _payslip(slip),
      ],
      const SizedBox(height: 20),
      Text('What this posts', style: Theme.of(context).textTheme.titleMedium),
      const SizedBox(height: 8),
      // the four facts the journal writes, in the terms of the business: what the work cost, what the staff are owed, and
      // what is being held for somebody else. The accounts are found by name in this Book's chart, so they are not listed
      // by number here - a chart that numbers its own accounts would make the numbers meaningless to read.
      Text(
        'The work cost ${run.costToCompany.toDecimalString()}, which is '
        '${run.gross.toDecimalString()} of wages and '
        '${run.employerContributions.toDecimalString()} the business adds on top. '
        '${run.net.toDecimalString()} is owed to the staff, and '
        '${(run.paye + run.nassa + run.pension + run.medical + run.other + run.employerContributions).toDecimalString()} '
        'is held for the tax authority, the pension fund and anybody else the payslips withhold for.',
        style: Theme.of(context).textTheme.bodyMedium,
      ),
      const SizedBox(height: 6),
      Text(
        'Dated the last day of ${run.period}. No money moves: the wages that were paid belong in the cash book.',
        style: Theme.of(context).textTheme.bodySmall,
      ),
      if (!run.balanced)
        Padding(
          padding: const EdgeInsets.only(top: 8),
          child: Text(
            'This entry does not add up, so it will not be posted.',
            style: Theme.of(context).textTheme.bodyMedium,
          ),
        ),
      const SizedBox(height: 12),
      ElevatedButton(
        onPressed: (_posted || _posting || !run.balanced) ? null : _postTheRun,
        child: Text(
          _posted
              ? 'In the books'
              : _posting
              ? 'Posting'
              : 'Post it to the books',
        ),
      ),
      // the engine's answer, not a second sum: the net owed and the total cost are worked out where the entry is posted
      if (_postedRun != null) ...[
        const SizedBox(height: 16),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'The books now say',
                  style: Theme.of(context).textTheme.titleSmall,
                ),
                const SizedBox(height: 6),
                Text(
                  '${_postedRun!.period} · reference ${_postedRun!.reference}, dated '
                  '${_postedRun!.entryDate}. Gross wages ${_postedRun!.grossWages}, '
                  '${_postedRun!.withheld} withheld, ${_postedRun!.netPay} owed to staff.',
                ),
                const SizedBox(height: 4),
                Text(
                  'Employing people cost ${_postedRun!.totalCost} once the employer\'s own '
                  '${_postedRun!.employerContributions} is counted. The wages sit in account '
                  '${_postedRun!.wagesAccount}, what is held for others in '
                  '${_postedRun!.deductionsAccount}, and what the staff are owed in '
                  '${_postedRun!.netAccount}.',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ),
          ),
        ),
      ],
    ],
  );

  Widget _payslip(Payslip slip) => Card(
    margin: const EdgeInsets.only(bottom: 8),
    child: Padding(
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(slip.name, style: Theme.of(context).textTheme.titleSmall),
          const SizedBox(height: 4),
          Text(
            'Gross ${slip.gross.toDecimalString()}, '
            '${slip.deductions.toDecimalString()} off, '
            'takes home ${slip.net.toDecimalString()}',
          ),
          if (!slip.deductions.isZero)
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Text(
                'PAYE ${slip.paye.toDecimalString()} · NSSA ${slip.nassa.toDecimalString()} · '
                'pension ${slip.pension.toDecimalString()} · medical ${slip.medical.toDecimalString()}'
                '${slip.other.isZero ? '' : ' · other ${slip.other.toDecimalString()}'}',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ),
        ],
      ),
    ),
  );

  Widget _figure(String label, ExactAmount amount) => Padding(
    padding: const EdgeInsets.only(bottom: 4),
    child: Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [Text(label), Text(amount.toDecimalString())],
    ),
  );
}
