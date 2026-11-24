// Money already paid for something the business has not used yet.
//
// A year of insurance paid in one go in January is not January's cost: eleven twelfths of it buys something the business
// will use later. Charging all of it to January makes that month look far worse than it was and every month after it look
// better than it was, and the balance sheet loses an asset the business actually holds.
//
// So the money goes to a prepaid asset, and the cash book records it leaving the account in the same act, because the money
// really did leave. Each month then takes its share out of the asset and into the cost. What is left is what is still to be
// used, which is stated plainly on each one.
//
// The months are split by the engine, exactly, with the odd cent going to the earliest months: a year of 1,000.00 is 83.34
// then 83.33 eleven times, adding back to what was paid. This screen never divides anything.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/account_picker_sheet.dart';
import 'package:vimbai_mobile_client/features/accruals/data/accrual_posting.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';

class PaidUpFrontPage extends StatefulWidget {
  const PaidUpFrontPage({super.key, this.posting, this.ledger, this.cashbook});

  /// A test passes its own services so the page can be exercised without a server.
  final AccrualPostingService? posting;
  final AccountingApiService? ledger;
  final CashBookService? cashbook;

  @override
  State<PaidUpFrontPage> createState() => _PaidUpFrontPageState();
}

class _PaidUpFrontPageState extends State<PaidUpFrontPage> {
  late final AccrualPostingService _posting =
      widget.posting ?? AccrualPostingService.instance;
  late final AccountingApiService _ledger =
      widget.ledger ?? AccountingApiService();
  late final CashBookService _cashbook = widget.cashbook ?? CashBookService();

  final TextEditingController _reference = TextEditingController();
  final TextEditingController _total = TextEditingController();
  final TextEditingController _months = TextEditingController(text: '12');
  final TextEditingController _coversFrom = TextEditingController(
    text: _thisMonth(),
  );
  final TextEditingController _what = TextEditingController();

  List<Account> _costAccounts = const [];
  List<Account> _moneyAccounts = const [];
  List<CashAccount> _held = const [];
  List<Prepayment> _recorded = const [];

  String _expenseAccount = '';
  String _moneyAccount = '';
  String _paidFrom = '';
  bool _reading = true;
  bool _saving = false;
  String? _trouble;

  @override
  void initState() {
    super.initState();
    _read();
  }

  @override
  void dispose() {
    _reference.dispose();
    _total.dispose();
    _months.dispose();
    _coversFrom.dispose();
    _what.dispose();
    super.dispose();
  }

  static String _thisMonth() {
    final today = DateTime.now();
    return '${today.year}-${today.month.toString().padLeft(2, '0')}';
  }

  Future<void> _read() async {
    setState(() {
      _reading = true;
      _trouble = null;
    });
    try {
      final chart = await _ledger.getAllAccounts();
      final recorded = await _posting.prepayments();
      if (!mounted) return;
      setState(() {
        _costAccounts = [
          for (final account in chart)
            if (account.accountType == 'expense') account,
        ]..sort((a, b) => a.accountNumber.compareTo(b.accountNumber));
        _moneyAccounts = [
          for (final account in chart)
            if (account.accountType == 'asset') account,
        ]..sort((a, b) => a.accountNumber.compareTo(b.accountNumber));
        _recorded = recorded;
        _reading = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _reading = false;
        _trouble = CouldNotRead.reasonOf(failure);
      });
    }
    try {
      final accounts = await _cashbook.accounts();
      if (!mounted) return;
      setState(() {
        _held = accounts;
        if (_paidFrom.isEmpty && accounts.length == 1) {
          _paidFrom = accounts.first.code;
        }
      });
    } catch (_) {
      // a Book whose cash accounts could not be read cannot record a payment, which is what the refusal below says
    }
  }

  ExactAmount? get _figure {
    final typed = _total.text.trim();
    if (typed.isEmpty) return null;
    try {
      return ExactAmount.parse(typed);
    } catch (_) {
      return null;
    }
  }

  int get _howManyMonths => int.tryParse(_months.text.trim()) ?? 0;

  /// Everything that would stop the engine posting, said before it is asked.
  String? get _refusal {
    if (_reference.text.trim().isEmpty) {
      return 'Give it a reference of your own, so the same one cannot be recorded twice.';
    }
    final figure = _figure;
    if (figure == null || figure.isZero) {
      return 'Say what was paid.';
    }
    if (_howManyMonths < 1) {
      return 'Say how many months the cover runs for, so each month can take its share.';
    }
    if (!RegExp(r'^\d{4}-\d{2}$').hasMatch(_coversFrom.text.trim())) {
      return 'Say which month the cover starts, as 2026-01.';
    }
    if (_expenseAccount.isEmpty) {
      return 'Say which cost this becomes as it is used up.';
    }
    if (_paidFrom.isEmpty) {
      return 'Say which cash or bank account the money left.';
    }
    if (_moneyAccount.isEmpty) {
      return 'Say which account in the chart the money sits in.';
    }
    return null;
  }

  Future<void> _record() async {
    final refusal = _refusal;
    if (refusal != null) {
      setState(() => _trouble = refusal);
      return;
    }
    setState(() {
      _saving = true;
      _trouble = null;
    });
    try {
      await _posting.recordPrepayment(
        reference: _reference.text,
        total: _figure!,
        months: _howManyMonths,
        coversFrom: _coversFrom.text,
        paidFrom: _paidFrom,
        moneyAccount: _moneyAccount,
        expenseAccount: _expenseAccount,
        description: _what.text,
      );
      if (!mounted) return;
      _reference.clear();
      _total.clear();
      _what.clear();
      setState(() => _saving = false);
      await _read();
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _trouble = CouldNotRead.reasonOf(failure);
      });
    }
  }

  Future<void> _useUp(Prepayment prepayment) async {
    final month = prepayment.nextMonth;
    if (month.isEmpty) return;
    setState(() {
      _saving = true;
      _trouble = null;
    });
    try {
      await _posting.releaseMonth(prepaymentId: prepayment.id, period: month);
      if (!mounted) return;
      setState(() => _saving = false);
      await _read();
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _trouble = CouldNotRead.reasonOf(failure);
      });
    }
  }

  String _costName(String number) {
    for (final account in _costAccounts) {
      if (account.accountNumber == number) return account.accountName;
    }
    return number;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Paid up front')),
      body: _reading
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
              children: [
                Text(
                  'Money already paid for something used over several months is not one month\'s cost. It is kept as '
                  'something the business still has, and each month takes its share out of it.',
                  style: theme.textTheme.bodyMedium,
                ),
                const SizedBox(height: 16),
                TextField(
                  controller: _reference,
                  decoration: const InputDecoration(
                    labelText: 'Your reference',
                    helperText:
                        'The policy number, or whatever you would call it.',
                    border: OutlineInputBorder(),
                  ),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _total,
                  keyboardType: const TextInputType.numberWithOptions(
                    decimal: true,
                  ),
                  decoration: const InputDecoration(
                    labelText: 'What was paid',
                    border: OutlineInputBorder(),
                  ),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _months,
                  keyboardType: TextInputType.number,
                  decoration: const InputDecoration(
                    labelText: 'How many months it covers',
                    border: OutlineInputBorder(),
                  ),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _coversFrom,
                  decoration: const InputDecoration(
                    labelText: 'The month the cover starts',
                    helperText: 'As 2026-01. Its own month is the first share.',
                    border: OutlineInputBorder(),
                  ),
                ),
                const SizedBox(height: 12),
                InkWell(
                  onTap: () async {
                    final picked = await showAccountPicker(
                      context,
                      accounts: _costAccounts,
                    );
                    if (picked == null || !mounted) return;
                    setState(() => _expenseAccount = picked.accountNumber);
                  },
                  child: InputDecorator(
                    decoration: const InputDecoration(
                      labelText: 'What it becomes as it is used',
                      helperText: 'The cost each month takes its share into.',
                      border: OutlineInputBorder(),
                    ),
                    child: Text(
                      _expenseAccount.isEmpty
                          ? 'Choose the cost'
                          : '$_expenseAccount · ${_costName(_expenseAccount)}',
                    ),
                  ),
                ),
                const SizedBox(height: 12),
                DropdownButtonFormField<String>(
                  initialValue: _paidFrom.isEmpty ? null : _paidFrom,
                  decoration: const InputDecoration(
                    labelText: 'Paid from',
                    border: OutlineInputBorder(),
                  ),
                  items: [
                    for (final account in _held)
                      DropdownMenuItem(
                        value: account.code,
                        child: Text('Paid from ${account.name}'),
                      ),
                  ],
                  onChanged: _saving
                      ? null
                      : (value) => setState(() => _paidFrom = value ?? ''),
                ),
                const SizedBox(height: 12),
                DropdownButtonFormField<String>(
                  initialValue: _moneyAccount.isEmpty ? null : _moneyAccount,
                  decoration: const InputDecoration(
                    labelText: 'The account the money sits in',
                    helperText:
                        'The ledger credits this one; the cash book moves the other.',
                    border: OutlineInputBorder(),
                  ),
                  items: [
                    for (final account in _moneyAccounts)
                      DropdownMenuItem(
                        value: account.accountNumber,
                        child: Text(
                          '${account.accountNumber} · ${account.accountName}',
                        ),
                      ),
                  ],
                  onChanged: _saving
                      ? null
                      : (value) => setState(() => _moneyAccount = value ?? ''),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _what,
                  decoration: const InputDecoration(
                    labelText: 'What it was for',
                    helperText:
                        'Optional: insurance, rent in advance, a licence.',
                    border: OutlineInputBorder(),
                  ),
                ),
                const SizedBox(height: 20),
                FilledButton(
                  onPressed: _saving ? null : _record,
                  child: Text(
                    _saving ? 'Recording...' : 'Record what was paid',
                  ),
                ),
                if (_trouble != null) ...[
                  const SizedBox(height: 16),
                  Text(
                    _trouble!,
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: theme.colorScheme.error,
                    ),
                  ),
                ],
                const SizedBox(height: 28),
                Text('Still to be used', style: theme.textTheme.titleSmall),
                const SizedBox(height: 6),
                if (_recorded.isEmpty)
                  Text(
                    'Nothing has been paid up front yet. Anything recorded here appears until its months are used up.',
                    style: theme.textTheme.bodySmall,
                  ),
                for (final prepayment in _recorded)
                  Card(
                    child: Padding(
                      padding: const EdgeInsets.all(12),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            prepayment.description.trim().isEmpty
                                ? prepayment.reference
                                : '${prepayment.reference} · ${prepayment.description}',
                            style: theme.textTheme.titleSmall,
                          ),
                          const SizedBox(height: 4),
                          Text(
                            '${prepayment.stillToUse} still to be used of ${prepayment.total} paid, over '
                            '${prepayment.months} months from ${prepayment.coversFrom}.',
                            style: theme.textTheme.bodySmall,
                          ),
                          const SizedBox(height: 8),
                          if (prepayment.isUsedUp)
                            Text(
                              'Every month of it has been used up.',
                              style: theme.textTheme.bodySmall,
                            )
                          else
                            Align(
                              alignment: Alignment.centerRight,
                              child: TextButton(
                                onPressed: _saving
                                    ? null
                                    : () => _useUp(prepayment),
                                child: Text(
                                  'Use up ${prepayment.nextMonth} · ${prepayment.nextAmount}',
                                ),
                              ),
                            ),
                        ],
                      ),
                    ),
                  ),
              ],
            ),
    );
  }
}
