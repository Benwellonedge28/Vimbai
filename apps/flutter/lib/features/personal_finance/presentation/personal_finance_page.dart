// Personal finance: recurring bills/income, debts, investments and tax.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/personal_finance/data/personal_finance_service.dart';

/// What this page reads, so a test can drive its loading, failed and empty states without a server. The real thing is
/// the engine, through the service; nothing else about the page changes when it is swapped.
abstract class PersonalFinanceReads {
  Future<List<Map<String, dynamic>>> recurring();
  Future<List<Map<String, dynamic>>> debts();
  Future<Map<String, dynamic>> portfolio();
}

class _EnginePersonalFinance implements PersonalFinanceReads {
  final _pf = PersonalFinanceService.instance;

  @override
  Future<List<Map<String, dynamic>>> recurring() => _pf.listRecurring();

  @override
  Future<List<Map<String, dynamic>>> debts() => _pf.listDebts();

  @override
  Future<Map<String, dynamic>> portfolio() => _pf.portfolio();
}

final PersonalFinanceReads _enginePersonalFinance = _EnginePersonalFinance();

class PersonalFinancePage extends StatefulWidget {
  const PersonalFinancePage({super.key, this.reads});

  /// Left out in the app; a test passes its own so a failed read can be watched.
  final PersonalFinanceReads? reads;

  @override
  State<PersonalFinancePage> createState() => _PersonalFinancePageState();
}

class _PersonalFinancePageState extends State<PersonalFinancePage> {
  final _pf = PersonalFinanceService.instance;
  int _section = 0; // 0 recurring, 1 debts, 2 investments, 3 tax

  List<Map<String, dynamic>> _recurring = [];
  List<Map<String, dynamic>> _debts = [];
  Map<String, dynamic> _portfolio = {};
  Map<String, dynamic>? _taxResult;

  /// A read that failed is its own state, not an empty list: showing "no debts yet" after a failed read tells a person
  /// their work is gone when it is merely unread, and a snackbar slides away while they are still reading it.
  bool _loading = true;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  PersonalFinanceReads get _reads => widget.reads ?? _enginePersonalFinance;

  Future<void> _reload() async {
    if (mounted) {
      setState(() {
        _loading = true;
        _error = null;
      });
    }
    try {
      if (_section == 0) {
        _recurring = await _reads.recurring();
      } else if (_section == 1) {
        _debts = await _reads.debts();
      } else if (_section == 2) {
        final j = await _reads.portfolio();
        _portfolio = j;
      }
    } catch (e) {
      _error = e;
    }
    if (mounted) {
      setState(() {
        _loading = false;
      });
    }
  }

  /// Loading, then what went wrong, then the list: in that order, because an empty list means something different once a
  /// read has answered.
  Widget _read(String what, Widget whenEmpty, Widget Function() whenRead) {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(what: what, error: _error, onRetry: _reload);
    }
    return whenRead();
  }

  void _say(String message) {
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text(message)));
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Personal finance')),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _section,
        onDestinationSelected: (i) {
          _section = i;
          _taxResult = null;
          _reload();
        },
        destinations: const [
          NavigationDestination(icon: Icon(Icons.repeat), label: 'Recurring'),
          NavigationDestination(
            icon: Icon(Icons.account_balance),
            label: 'Debts',
          ),
          NavigationDestination(
            icon: Icon(Icons.trending_up),
            label: 'Investments',
          ),
          NavigationDestination(icon: Icon(Icons.calculate), label: 'Tax'),
        ],
      ),
      body: _section == 0
          ? _recurringView()
          : _section == 1
          ? _debtsView()
          : _section == 2
          ? _investmentsView()
          : _taxView(),
      floatingActionButton: _section == 0
          ? FloatingActionButton(
              heroTag: 'recurring-add',
              onPressed: _addRecurring,
              child: const Icon(Icons.add),
            )
          : _section == 1
          ? FloatingActionButton(
              heroTag: 'debt-add',
              onPressed: _addDebt,
              child: const Icon(Icons.add),
            )
          : _section == 2
          ? FloatingActionButton(
              heroTag: 'inv-add',
              onPressed: _addInvestment,
              child: const Icon(Icons.add),
            )
          : null,
    );
  }

  Widget _recurringView() {
    return _read(
      'Your recurring transactions',
      const Center(child: Text('No recurring transactions yet.')),
      _recurringList,
    );
  }

  Widget _recurringList() {
    if (_recurring.isEmpty) {
      return const Center(child: Text('No recurring transactions yet.'));
    }
    return ListView.builder(
      itemCount: _recurring.length,
      itemBuilder: (ctx, i) {
        final r = _recurring[i];
        final due = r['due'] == true;
        return ListTile(
          leading: Icon(
            r['kind'] == 'income' ? Icons.south_west : Icons.north_east,
            color: due ? Colors.orange : null,
          ),
          title: Text(r['description']),
          subtitle: Text(
            '${r['currency']} ${r['amount']} - ${r['frequency']}'
            ' - next ${r['next_due']}',
          ),
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (due)
                TextButton(
                  onPressed: () async {
                    await _pf.runRecurring(r['id'] as String);
                    _reload();
                  },
                  child: const Text('Record'),
                ),
              IconButton(
                icon: const Icon(Icons.delete_outline),
                onPressed: () async {
                  await _pf.deleteRecurring(r['id'] as String);
                  _reload();
                },
              ),
            ],
          ),
        );
      },
    );
  }

  Future<void> _addRecurring() async {
    final desc = TextEditingController();
    final amount = TextEditingController();
    final due = TextEditingController(
      text: DateTime.now().toIso8601String().substring(0, 10),
    );
    String kind = 'bill';
    String freq = 'monthly';
    await showDialog(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialog) => AlertDialog(
          title: const Text('New recurring transaction'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              SegmentedButton<String>(
                segments: const [
                  ButtonSegment(value: 'bill', label: Text('Bill')),
                  ButtonSegment(value: 'income', label: Text('Income')),
                ],
                selected: {kind},
                onSelectionChanged: (s) => setDialog(() => kind = s.first),
              ),
              TextField(
                controller: desc,
                decoration: const InputDecoration(labelText: 'Description'),
              ),
              TextField(
                controller: amount,
                decoration: const InputDecoration(labelText: 'Amount'),
                keyboardType: TextInputType.number,
              ),
              DropdownButtonFormField<String>(
                value: freq,
                items: ['weekly', 'monthly', 'quarterly', 'yearly']
                    .map((f) => DropdownMenuItem(value: f, child: Text(f)))
                    .toList(),
                onChanged: (v) => setDialog(() => freq = v ?? freq),
                decoration: const InputDecoration(labelText: 'Frequency'),
              ),
              TextField(
                controller: due,
                decoration: const InputDecoration(
                  labelText: 'Next due (YYYY-MM-DD)',
                ),
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () {
                Navigator.pop(ctx);
                final typed = ExactAmount.tryParse(amount.text);
                if (typed == null) {
                  _say('Amount must be a number, such as 250');
                  return;
                }
                _pf
                    .createRecurring(
                      kind: kind,
                      description: desc.text,
                      amount: typed,
                      frequency: freq,
                      nextDue: due.text,
                    )
                    .then((_) => _reload());
              },
              child: const Text('Save'),
            ),
          ],
        ),
      ),
    );
  }

  Widget _debtsView() {
    return _read(
      'Your debts',
      const Center(child: Text('No debts tracked yet.')),
      _debtsList,
    );
  }

  Widget _debtsList() {
    if (_debts.isEmpty) {
      return const Center(child: Text('No debts tracked yet.'));
    }
    return ListView.builder(
      itemCount: _debts.length,
      itemBuilder: (ctx, i) {
        final d = _debts[i];
        final s = d['state'] as Map<String, dynamic>;
        return ListTile(
          leading: const Icon(Icons.account_balance),
          title: Text(d['name']),
          subtitle: Text(
            'Balance ${s['balance']} - pay ${s['scheduled_monthly_payment']}/mo'
            ' - ${s['months_remaining']} months left',
          ),
          trailing: TextButton(
            onPressed: () => _payDebt(d['id'] as String, d['name'] as String),
            child: const Text('Pay'),
          ),
        );
      },
    );
  }

  Future<void> _payDebt(String id, String name) async {
    final amount = TextEditingController();
    await showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Payment - $name'),
        content: TextField(
          controller: amount,
          decoration: const InputDecoration(labelText: 'Amount'),
          keyboardType: TextInputType.number,
        ),
        actions: [
          TextButton(
            onPressed: () {
              Navigator.pop(ctx);
              final typed = ExactAmount.tryParse(amount.text);
              if (typed == null) {
                _say('Amount must be a number, such as 250');
                return;
              }
              _pf.payDebt(id, typed).then((_) => _reload());
            },
            child: const Text('Record'),
          ),
        ],
      ),
    );
  }

  Future<void> _addDebt() async {
    final name = TextEditingController();
    final principal = TextEditingController();
    final rate = TextEditingController(text: '12');
    final term = TextEditingController(text: '24');
    await showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('New debt'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: name,
              decoration: const InputDecoration(labelText: 'Name'),
            ),
            TextField(
              controller: principal,
              decoration: const InputDecoration(labelText: 'Principal'),
              keyboardType: TextInputType.number,
            ),
            TextField(
              controller: rate,
              decoration: const InputDecoration(labelText: 'Annual rate %'),
              keyboardType: TextInputType.number,
            ),
            TextField(
              controller: term,
              decoration: const InputDecoration(labelText: 'Term (months)'),
              keyboardType: TextInputType.number,
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () {
              final typed = ExactAmount.tryParse(principal.text);
              if (typed == null) {
                _say('Principal must be a number, such as 5000');
                return;
              }
              Navigator.pop(ctx);
              _pf
                  .createDebt(
                    name: name.text,
                    principal: typed,
                    annualRate: double.tryParse(rate.text) ?? 0,
                    termMonths: int.tryParse(term.text) ?? 1,
                    startedAt: DateTime.now().toIso8601String().substring(
                      0,
                      10,
                    ),
                  )
                  .then((_) => _reload());
            },
            child: const Text('Save'),
          ),
        ],
      ),
    );
  }

  Widget _investmentsView() {
    return _read(
      'Your investments',
      const Center(child: Text('No investments yet.')),
      _investmentsList,
    );
  }

  Widget _investmentsList() {
    final invs = (_portfolio['investments'] as List? ?? [])
        .cast<Map<String, dynamic>>();
    final p = _portfolio['portfolio'] as Map<String, dynamic>? ?? {};
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.all(12),
          child: Text(
            'Portfolio: ${p['market_value'] ?? 0} '
            '(gain ${p['unrealized_gain'] ?? 0})',
            style: const TextStyle(fontWeight: FontWeight.bold),
          ),
        ),
        Expanded(
          child: invs.isEmpty
              ? const Center(child: Text('No investments yet.'))
              : ListView.builder(
                  itemCount: invs.length,
                  itemBuilder: (ctx, i) {
                    final iv = invs[i];
                    final gain = iv['unrealized_gain'] as num? ?? 0;
                    return ListTile(
                      leading: const Icon(Icons.trending_up),
                      title: Text(iv['name']),
                      subtitle: Text(
                        '${iv['units']} units @ ${iv['avg_cost']} - '
                        'now ${iv['last_price']}',
                      ),
                      trailing: Text(
                        '$gain',
                        style: TextStyle(
                          color: gain >= 0 ? Colors.green : Colors.red,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                    );
                  },
                ),
        ),
      ],
    );
  }

  Future<void> _addInvestment() async {
    final name = TextEditingController();
    final units = TextEditingController();
    final price = TextEditingController();
    await showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('New investment'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: name,
              decoration: const InputDecoration(labelText: 'Name'),
            ),
            TextField(
              controller: units,
              decoration: const InputDecoration(labelText: 'Units'),
              keyboardType: TextInputType.number,
            ),
            TextField(
              controller: price,
              decoration: const InputDecoration(labelText: 'Price per unit'),
              keyboardType: TextInputType.number,
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () {
              final typed = ExactAmount.tryParse(price.text);
              if (typed == null) {
                _say('Price must be a number, such as 12.50');
                return;
              }
              Navigator.pop(ctx);
              _pf
                  .createInvestment(
                    name: name.text,
                    initialUnits: double.tryParse(units.text) ?? 0,
                    initialPrice: typed,
                  )
                  .then((_) => _reload());
            },
            child: const Text('Save'),
          ),
        ],
      ),
    );
  }

  Widget _taxView() {
    final income = TextEditingController();
    final deductions = TextEditingController();
    final paye = TextEditingController();
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        const Text(
          'Estimate your annual tax',
          style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
        ),
        const SizedBox(height: 12),
        TextField(
          controller: income,
          decoration: const InputDecoration(labelText: 'Annual income'),
          keyboardType: TextInputType.number,
        ),
        TextField(
          controller: deductions,
          decoration: const InputDecoration(labelText: 'Deductions'),
          keyboardType: TextInputType.number,
        ),
        TextField(
          controller: paye,
          decoration: const InputDecoration(labelText: 'PAYE already paid'),
          keyboardType: TextInputType.number,
        ),
        const SizedBox(height: 12),
        ElevatedButton(
          onPressed: () async {
            // A blank field means nothing claimed; anything else that is not a number is refused, not counted as nothing.
            ExactAmount? read(TextEditingController field) =>
                field.text.trim().isEmpty
                ? ExactAmount.zero
                : ExactAmount.tryParse(field.text);
            final earned = read(income);
            final claimed = read(deductions);
            final paid = read(paye);
            if (earned == null || claimed == null || paid == null) {
              _say('Income, deductions and PAYE paid must be numbers');
              return;
            }
            final j = await _pf.estimateTax(
              annualIncome: earned,
              deductions: claimed,
              payePaid: paid,
            );
            setState(() => _taxResult = j);
          },
          child: const Text('Estimate'),
        ),
        if (_taxResult != null) ...[
          const SizedBox(height: 16),
          Text('Taxable income: ${_taxResult!['taxable_income']}'),
          Text('Estimated tax: ${_taxResult!['estimated_tax']}'),
          Text('Effective rate: ${_taxResult!['effective_rate']}'),
          Text('Monthly withholding: ${_taxResult!['monthly_withholding']}'),
          Text('Balance due: ${_taxResult!['balance_due']}'),
        ],
      ],
    );
  }
}
