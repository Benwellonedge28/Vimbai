// A cost a month has used and nobody has billed yet.
//
// The electricity for November is used in November and the bill arrives in December. Left alone, November's profit looks
// better than the month really was and December carries a cost it did not cause, which is how a business comes to believe
// a good month was better than it was.
//
// The screen asks for three things a person knows without looking anything up: which month, what the cost was roughly, and
// what it was for. It then says what it is about to do, in the terms of the business rather than the ledger: the month takes
// the cost and the business owes it.
//
// The estimate is an estimate, so it carries no tax: tax belongs to the document that charges it, and when the bill arrives
// its tax is reclaimed there, once. Taking an estimate back is a reversing entry and never an edit, so the month stays as it
// was believed and the reversal stands beside it, which is why the list below shows both.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/account_picker_sheet.dart';
import 'package:vimbai_mobile_client/features/accruals/data/accrual_posting.dart';

class ACostNotYetBilledPage extends StatefulWidget {
  const ACostNotYetBilledPage({super.key, this.posting, this.ledger});

  /// A test passes its own services so the page can be exercised without a server.
  final AccrualPostingService? posting;
  final AccountingApiService? ledger;

  @override
  State<ACostNotYetBilledPage> createState() => _ACostNotYetBilledPageState();
}

class _ACostNotYetBilledPageState extends State<ACostNotYetBilledPage> {
  late final AccrualPostingService _posting =
      widget.posting ?? AccrualPostingService.instance;
  late final AccountingApiService _ledger =
      widget.ledger ?? AccountingApiService();

  final TextEditingController _amount = TextEditingController();

  /// The month as typed, so the field keeps its cursor and its text between rebuilds.
  final TextEditingController _monthField = TextEditingController(
    text: _lastMonth(),
  );
  final TextEditingController _what = TextEditingController();

  List<Account> _costAccounts = const [];
  List<Accrual> _recorded = const [];
  String _account = '';
  String get _month => _monthField.text;
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
    _amount.dispose();
    _monthField.dispose();
    _what.dispose();
    super.dispose();
  }

  /// The month just gone, as `2026-11`: a cost is accrued once the month it belongs to is over or nearly so.
  static String _lastMonth() {
    final today = DateTime.now();
    final last = DateTime(today.year, today.month - 1, 1);
    return '${last.year}-${last.month.toString().padLeft(2, '0')}';
  }

  Future<void> _read() async {
    setState(() {
      _reading = true;
      _trouble = null;
    });
    try {
      final chart = await _ledger.getAllAccounts();
      final recorded = await _posting.accruals();
      if (!mounted) return;
      setState(() {
        _costAccounts = [
          for (final account in chart)
            if (account.accountType == 'expense') account,
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
  }

  ExactAmount? get _figure {
    final typed = _amount.text.trim();
    if (typed.isEmpty) return null;
    try {
      return ExactAmount.parse(typed);
    } catch (_) {
      return null;
    }
  }

  /// Everything that would stop the engine posting, said before it is asked.
  String? get _refusal {
    if (!RegExp(r'^\d{4}-\d{2}$').hasMatch(_month.trim())) {
      return 'Say which month this cost belongs to, as 2026-11.';
    }
    if (_account.isEmpty) {
      return 'Say which cost this is, so it lands in the right part of the accounts.';
    }
    final figure = _figure;
    if (figure == null || figure.isZero) {
      return 'Say what the month used, as a figure.';
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
      await _posting.recordAccrual(
        period: _month.trim(),
        expenseAccount: _account,
        amount: _figure!,
        description: _what.text,
      );
      if (!mounted) return;
      _amount.clear();
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

  Future<void> _takeBack(Accrual accrual) async {
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Take the estimate back?'),
        content: Text(
          'The ${accrual.period} estimate of ${accrual.amount} stays exactly as it was recorded, and a reversal is '
          'posted dated the first day of the next month. That is how a correction is made: nothing is edited, so the '
          'month still shows what was believed at the time.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Leave it'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Take it back'),
          ),
        ],
      ),
    );
    if (sure != true || !mounted) return;
    setState(() {
      _saving = true;
      _trouble = null;
    });
    try {
      await _posting.reverseAccrual(accrual.id);
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

  /// The chosen cost as the chart names it, so the field reads the way the chart does rather than as a bare number.
  String get _chosenCost {
    if (_account.isEmpty) return 'Choose the cost';
    for (final account in _costAccounts) {
      if (account.accountNumber == _account) {
        return '${account.accountNumber} · ${account.accountName}';
      }
    }
    return _account;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final standing = _recorded.where((accrual) => accrual.isStanding).toList();
    return Scaffold(
      appBar: AppBar(title: const Text('A cost not yet billed')),
      body: _reading
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
              children: [
                Text(
                  'A month that has used something and not been billed for it takes the cost now and owes it, so its '
                  'profit is true. When the bill arrives it is posted as any bill is, and this estimate is taken back.',
                  style: theme.textTheme.bodyMedium,
                ),
                const SizedBox(height: 16),
                TextField(
                  controller: _monthField,
                  decoration: const InputDecoration(
                    labelText: 'Which month',
                    helperText: 'As 2026-11, the month that used the cost.',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) => setState(() {}),
                ),
                const SizedBox(height: 12),
                InkWell(
                  onTap: () async {
                    final picked = await showAccountPicker(
                      context,
                      accounts: _costAccounts,
                    );
                    if (picked == null || !mounted) return;
                    setState(() => _account = picked.accountNumber);
                  },
                  child: InputDecorator(
                    decoration: const InputDecoration(
                      labelText: 'What the cost was',
                      helperText:
                          'Electricity, rent, airtime: where it belongs.',
                      border: OutlineInputBorder(),
                    ),
                    child: Text(_chosenCost),
                  ),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _amount,
                  keyboardType: const TextInputType.numberWithOptions(
                    decimal: true,
                  ),
                  decoration: const InputDecoration(
                    labelText: 'What the month used',
                    helperText:
                        'An estimate is enough: the bill settles it later.',
                    border: OutlineInputBorder(),
                  ),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _what,
                  decoration: const InputDecoration(
                    labelText: 'What it was for',
                    helperText:
                        'Optional: it rides on the entry so the month reads plainly.',
                    border: OutlineInputBorder(),
                  ),
                ),
                const SizedBox(height: 20),
                FilledButton(
                  onPressed: _saving ? null : _record,
                  child: Text(
                    _saving ? 'Recording...' : 'Charge the month and owe it',
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
                Text('Still standing', style: theme.textTheme.titleSmall),
                const SizedBox(height: 6),
                if (standing.isEmpty)
                  Text(
                    'No month is carrying an estimate. When one is recorded it appears here until it is taken back.',
                    style: theme.textTheme.bodySmall,
                  ),
                for (final accrual in standing)
                  Card(
                    child: ListTile(
                      title: Text(
                        '${accrual.period} · ${accrual.description.trim().isEmpty ? 'Accrued cost' : accrual.description}',
                      ),
                      subtitle: Text(
                        '${accrual.amount} owed, charged to ${accrual.expenseAccount}',
                      ),
                      trailing: TextButton(
                        onPressed: _saving ? null : () => _takeBack(accrual),
                        child: const Text('Take it back'),
                      ),
                    ),
                  ),
                if (_recorded.any((accrual) => !accrual.isStanding)) ...[
                  const SizedBox(height: 20),
                  Text('Taken back', style: theme.textTheme.titleSmall),
                  const SizedBox(height: 6),
                  for (final accrual in _recorded.where(
                    (one) => !one.isStanding,
                  ))
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      dense: true,
                      title: Text('${accrual.period} · ${accrual.amount}'),
                      subtitle: Text('Reversed on ${accrual.reversedOn}'),
                    ),
                ],
              ],
            ),
    );
  }
}
