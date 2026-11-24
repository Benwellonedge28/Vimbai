// Recording a general expense: the ordinary costs of being open, paid or still owed.
//
// The screen exists because the costs that keep a business running had no door of their own. A purchase records stock; a
// bill records a supplier's invoice; a cash book entry records money moving but asks for an account code nobody remembers.
// Rent, fuel, airtime, wages and bank charges fell between all three.
//
// What it writes is decided by one question the person answers plainly: was it paid, or is it owed?
//
// * paid: the engine credits the account the money sits in and moves the cash book for the account the money left, in one
//   act, so neither record can drift from the other;
// * owed: it credits accounts payable and touches no cash at all, because a cost incurred is not a cost paid.
//
// The figure asked for is what was actually spent, tax included, because that is what the receipt says. The tax inside it
// comes out of that figure rather than being added on top, so the same amount is reclaimed as was charged.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/banking/data/currencies.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/expenses/data/expense_posting.dart';

class RecordExpensePage extends StatefulWidget {
  const RecordExpensePage({
    super.key,
    this.posting,
    this.ledger,
    this.cashbook,
    this.currencies,
  });

  /// A test passes its own services so the page can be exercised without a server.
  final ExpensePostingService? posting;
  final AccountingApiService? ledger;
  final CashBookService? cashbook;
  final CurrencyService? currencies;

  @override
  State<RecordExpensePage> createState() => _RecordExpensePageState();
}

class _RecordExpensePageState extends State<RecordExpensePage> {
  late final ExpensePostingService _posting =
      widget.posting ?? ExpensePostingService.instance;
  late final AccountingApiService _ledger =
      widget.ledger ?? AccountingApiService();
  late final CashBookService _cashbook = widget.cashbook ?? CashBookService();
  late final CurrencyService _currencies =
      widget.currencies ?? CurrencyService.instance;

  final TextEditingController _reference = TextEditingController();
  final TextEditingController _payee = TextEditingController();
  final TextEditingController _total = TextEditingController();
  final TextEditingController _tax = TextEditingController();
  final TextEditingController _rate = TextEditingController();
  final TextEditingController _description = TextEditingController();
  final TextEditingController _notes = TextEditingController();

  DateTime _spentOn = DateTime.now();

  /// Where the cost lands. Empty until the chart is read and a person picks one.
  String _expenseAccount = '';

  /// Empty means the cost has not been paid yet, which is a liability rather than a payment.
  String _paidFrom = '';

  /// The chart account the money sits in, chosen beside the cash or bank account it left.
  String _moneyAccount = '';

  /// The tax account, when the cost carries tax inside it.
  String _taxAccount = '';

  /// Empty means this Book's own money, which is the ordinary case.
  String _currency = '';

  List<Account> _expenseAccounts = const [];
  List<Account> _moneyAccounts = const [];
  List<Account> _taxAccounts = const [];
  List<CashAccount> _held = const [];
  List<IsoCurrency> _currenciesHeld = const [];
  bool _reading = true;
  bool _saving = false;
  String? _complaint;
  String? _readError;

  @override
  void initState() {
    super.initState();
    _read();
  }

  @override
  void dispose() {
    for (final field in [
      _reference,
      _payee,
      _total,
      _tax,
      _rate,
      _description,
      _notes,
    ]) {
      field.dispose();
    }
    super.dispose();
  }

  Future<void> _read() async {
    setState(() {
      _reading = true;
      _readError = null;
    });
    try {
      // the Book's own chart says where costs belong and where money sits; nothing is typed as a number by hand
      final chart = await _ledger.getAllAccounts();
      if (!mounted) return;
      setState(() {
        _expenseAccounts = [
          for (final account in chart)
            if (account.accountType == 'expense') account,
        ]..sort((a, b) => a.accountNumber.compareTo(b.accountNumber));
        _moneyAccounts = [
          for (final account in chart)
            if (account.accountType == 'asset') account,
        ]..sort((a, b) => a.accountNumber.compareTo(b.accountNumber));
        _taxAccounts = [
          for (final account in chart)
            if (account.accountType != 'expense' &&
                (account.accountName.toLowerCase().contains('vat') ||
                    account.accountName.toLowerCase().contains('tax')))
              account,
        ]..sort((a, b) => a.accountNumber.compareTo(b.accountNumber));
        // a single tax account is the one the cost's tax belongs in, so it is proposed rather than asked for
        if (_taxAccounts.length == 1) {
          _taxAccount = _taxAccounts.first.accountNumber;
        }
        _reading = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _reading = false;
        _readError = CouldNotRead.reasonOf(failure);
      });
    }
    try {
      final accounts = await _cashbook.accounts();
      if (!mounted) return;
      setState(() => _held = accounts);
    } catch (_) {
      // a Book whose cash accounts could not be read can still record an expense it owes, which touches no cash
    }
    try {
      final held = await _currencies.read();
      if (!mounted) return;
      setState(() => _currenciesHeld = held.held);
    } catch (_) {
      // a Book whose currencies could not be read still records expenses in its own money
    }
  }

  ExactAmount? _figure(TextEditingController field) {
    final typed = field.text.trim();
    if (typed.isEmpty) return null;
    try {
      return ExactAmount.parse(typed);
    } catch (_) {
      return null;
    }
  }

  static String _day(DateTime date) =>
      '${date.year.toString().padLeft(4, '0')}-'
      '${date.month.toString().padLeft(2, '0')}-'
      '${date.day.toString().padLeft(2, '0')}';

  Future<void> _pickDay() async {
    final picked = await showDatePicker(
      context: context,
      initialDate: _spentOn,
      firstDate: DateTime(2000),
      lastDate: DateTime(2100),
    );
    if (picked == null || !mounted) return;
    setState(() => _spentOn = picked);
  }

  /// Everything that would stop the engine posting, said before it is asked.
  String? get _refusal {
    if (_expenseAccount.isEmpty) {
      return 'Say which account the cost belongs in, so it shows in the right '
          'part of the accounts.';
    }
    if (_reference.text.trim().isEmpty) {
      return 'Give the expense a reference of your own, so the same one cannot '
          'be recorded twice.';
    }
    final total = _figure(_total);
    if (total == null || total.isZero || total.isNegative) {
      return 'Say what was actually spent, which is the figure on the receipt.';
    }
    final tax = _figure(_tax);
    if (tax != null && tax > total) {
      return 'The tax cannot be bigger than what was spent.';
    }
    if (tax != null && !tax.isZero && _taxAccount.isEmpty) {
      return 'This Book has more than one account for tax, so say which one this '
          'cost\'s tax belongs in.';
    }
    if (_paidFrom.isNotEmpty && _moneyAccount.isEmpty) {
      return 'Say which account in the chart the money sits in, so the payment '
          'is posted against it as well as recorded in the cash book.';
    }
    if (_currency.isNotEmpty && (_figure(_rate)?.isZero ?? true)) {
      return 'Say what one $_currency is worth in this Book\'s money, so the '
          'cost is carried at a figure the books can use.';
    }
    return null;
  }

  Future<void> _post() async {
    final refusal = _refusal;
    if (refusal != null) {
      setState(() => _complaint = refusal);
      return;
    }
    setState(() {
      _saving = true;
      _complaint = null;
    });
    try {
      final recorded = await _posting.post(
        entryDate: _day(_spentOn),
        reference: _reference.text,
        expenseAccount: _expenseAccount,
        total: _figure(_total)!,
        paidTo: _payee.text,
        tax: _figure(_tax),
        taxAccount: _taxAccount,
        paidFrom: _paidFrom,
        moneyAccount: _moneyAccount,
        description: _description.text,
        currency: _currency,
        rate: _currency.isEmpty ? '' : _rate.text,
        notes: _notes.text,
      );
      if (!mounted) return;
      Navigator.of(context).pop(recorded);
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _complaint = CouldNotRead.reasonOf(failure);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final paid = _paidFrom.isNotEmpty;
    return Scaffold(
      appBar: AppBar(title: const Text('An expense')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            'Rent, fuel, airtime, wages, bank charges: the costs of being open '
            'that are not stock and not a supplier\'s bill.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 16),
          if (_reading) const LinearProgressIndicator(),
          if (_readError != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Text(
                'The chart of accounts could not be read, so the accounts to '
                'choose from are not here: $_readError',
                style: theme.textTheme.bodySmall,
              ),
            ),
          if (_expenseAccounts.isNotEmpty)
            DropdownButtonFormField<String>(
              initialValue: _expenseAccount.isEmpty ? null : _expenseAccount,
              decoration: const InputDecoration(
                labelText: 'What the cost was',
                helperText: 'Where this expense lands in the accounts.',
              ),
              items: [
                for (final account in _expenseAccounts)
                  DropdownMenuItem(
                    value: account.accountNumber,
                    child: Text(
                      '${account.accountNumber} · ${account.accountName}',
                    ),
                  ),
              ],
              onChanged: (picked) =>
                  setState(() => _expenseAccount = picked ?? ''),
            ),
          const SizedBox(height: 8),
          TextField(
            controller: _reference,
            decoration: const InputDecoration(
              labelText: 'Your reference for this expense',
              helperText:
                  'The receipt or voucher number, so it cannot be '
                  'entered twice.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _payee,
            decoration: const InputDecoration(
              labelText: 'Who it was paid to (if any)',
              helperText: 'The landlord, the filling station, the bank.',
            ),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _total,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'What was spent',
              helperText: 'The figure on the receipt, tax included.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _tax,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'Tax inside that figure (if any)',
              helperText:
                  'Comes out of what was spent rather than being added '
                  'to it, so the same amount is reclaimed as was charged.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          if (_taxAccounts.length > 1) ...[
            const SizedBox(height: 8),
            DropdownButtonFormField<String>(
              initialValue: _taxAccount.isEmpty ? null : _taxAccount,
              decoration: const InputDecoration(labelText: 'Which tax account'),
              items: [
                for (final account in _taxAccounts)
                  DropdownMenuItem(
                    value: account.accountNumber,
                    child: Text(
                      '${account.accountNumber} · ${account.accountName}',
                    ),
                  ),
              ],
              onChanged: (picked) => setState(() => _taxAccount = picked ?? ''),
            ),
          ],
          const SizedBox(height: 8),
          DropdownButtonFormField<String>(
            initialValue: _currency,
            decoration: const InputDecoration(
              labelText: 'Which money it was spent in',
            ),
            items: [
              const DropdownMenuItem(
                value: '',
                child: Text('This Book\'s own money'),
              ),
              for (final currency in _currenciesHeld)
                DropdownMenuItem(
                  value: currency.code,
                  child: Text('${currency.code} · ${currency.name}'),
                ),
            ],
            onChanged: (picked) => setState(() => _currency = picked ?? ''),
          ),
          if (_currency.isNotEmpty) ...[
            const SizedBox(height: 8),
            TextField(
              controller: _rate,
              keyboardType: const TextInputType.numberWithOptions(
                decimal: true,
              ),
              decoration: InputDecoration(
                labelText: 'Rate on the day it was spent',
                helperText:
                    'How much one $_currency is worth in this Book\'s money. '
                    'The engine converts at this rate and keeps what the '
                    'receipt showed.',
              ),
              onChanged: (_) => setState(() {}),
            ),
          ],
          const SizedBox(height: 8),
          ListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Spent on'),
            subtitle: Text(_day(_spentOn)),
            trailing: TextButton(
              onPressed: _pickDay,
              child: const Text('Change'),
            ),
          ),
          const SizedBox(height: 8),
          // one plain question decides what is written, because a cost incurred and a cost paid are not the same event
          DropdownButtonFormField<String>(
            initialValue: _paidFrom,
            decoration: const InputDecoration(
              labelText: 'Has it been paid?',
              helperText:
                  'Paid moves the account it left. Not yet paid is owed '
                  'instead, and touches no cash.',
            ),
            items: [
              const DropdownMenuItem(
                value: '',
                child: Text('Not paid yet: it is owed'),
              ),
              for (final account in _held)
                DropdownMenuItem(
                  value: account.code,
                  child: Text('Paid from ${account.name}'),
                ),
            ],
            onChanged: (picked) => setState(() => _paidFrom = picked ?? ''),
          ),
          if (paid) ...[
            const SizedBox(height: 8),
            DropdownButtonFormField<String>(
              initialValue: _moneyAccount.isEmpty ? null : _moneyAccount,
              decoration: const InputDecoration(
                labelText: 'The account that money sits in',
                helperText: 'What the ledger credits when the money leaves.',
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
              onChanged: (picked) =>
                  setState(() => _moneyAccount = picked ?? ''),
            ),
          ],
          const SizedBox(height: 8),
          TextField(
            controller: _description,
            decoration: const InputDecoration(
              labelText: 'What it was for (if any)',
              helperText: 'A few words for whoever reads the ledger.',
            ),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _notes,
            decoration: const InputDecoration(labelText: 'Notes (if any)'),
          ),
          const SizedBox(height: 16),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'What this will write',
                    style: theme.textTheme.titleSmall,
                  ),
                  const SizedBox(height: 4),
                  Text(
                    paid
                        ? 'The cost goes into the accounts, the tax inside it '
                              'goes to its own account to be reclaimed, and the '
                              'money the business holds falls by what was spent. '
                              'The cash book and the ledger are written together.'
                        : 'The cost goes into the accounts and what the business '
                              'owes goes up by the same. No cash or bank account '
                              'is touched, because nothing has been paid yet.',
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
            ),
          ),
          if (_complaint != null) ...[
            const SizedBox(height: 12),
            Text(
              _complaint!,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.error,
              ),
            ),
          ],
          const SizedBox(height: 16),
          FilledButton(
            onPressed: _saving ? null : _post,
            child: Text(_saving ? 'Recording…' : 'Record the expense'),
          ),
          const SizedBox(height: 24),
        ],
      ),
    );
  }
}
