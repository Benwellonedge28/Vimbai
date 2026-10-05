// Household finance: analyze a household's incomes, expenses, assets
// and liabilities for a period (household-finance-service).
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/services/household_finance_service.dart';

class HouseholdFinancePage extends StatefulWidget {
  const HouseholdFinancePage({super.key});

  @override
  State<HouseholdFinancePage> createState() => _HouseholdFinancePageState();
}

class _HouseholdFinancePageState extends State<HouseholdFinancePage> {
  final HouseholdFinanceService _hf = HouseholdFinanceService.instance;
  final _householdCtrl = TextEditingController(text: 'household');
  final _periodCtrl = TextEditingController(text: '2026-10');

  final List<Map<String, dynamic>> _incomes = [];
  final List<Map<String, dynamic>> _expenses = [];
  final List<Map<String, dynamic>> _assets = [];
  final List<Map<String, dynamic>> _liabilities = [];

  Map<String, dynamic>? _result;
  bool _busy = false;
  String? _error;

  static const _expenseCategories = [
    'housing', 'food', 'transport', 'utilities', 'education',
    'healthcare', 'entertainment', 'savings', 'other',
  ];
  static const _frequencies = ['weekly', 'monthly', 'quarterly', 'yearly'];

  Future<void> _analyze() async {
    setState(() { _busy = true; _error = null; });
    try {
      final r = await _hf.analyze(
        householdId: _householdCtrl.text.trim(),
        period: _periodCtrl.text.trim(),
        incomes: _incomes,
        expenses: _expenses,
        assets: _assets,
        liabilities: _liabilities,
      );
      if (mounted) setState(() => _result = r);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    }
    if (mounted) setState(() => _busy = false);
  }

  void _toast(String msg) =>
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(msg)));

  // -- row editors ------------------------------------------------------

  Future<void> _addIncome() async {
    final src = TextEditingController();
    final amt = TextEditingController();
    String freq = 'monthly';
    final ok = await _formDialog(
      title: 'Add income',
      fields: [
        (src, 'Source (e.g. salary, rental)', null),
        (amt, 'Amount', null),
      ],
      extra: (setState) => DropdownButton<String>(
        value: freq,
        isExpanded: true,
        items: _frequencies.map((f) => DropdownMenuItem(value: f, child: Text(f))).toList(),
        onChanged: (v) => setState(() => freq = v ?? freq),
      ),
    );
    if (ok != true) return;
    final amount = double.tryParse(amt.text.trim()) ?? 0;
    if (src.text.trim().isEmpty || amount <= 0) {
      _toast('Enter a source and a positive amount');
      return;
    }
    setState(() => _incomes.add({
      'source': src.text.trim(), 'amount': amount, 'frequency': freq,
    }));
  }

  Future<void> _addExpense() async {
    String category = 'food';
    final desc = TextEditingController();
    final amt = TextEditingController();
    String freq = 'monthly';
    final ok = await _formDialog(
      title: 'Add expense',
      fields: [
        (desc, 'Description', null),
        (amt, 'Amount', null),
      ],
      extra: (setState) => Column(mainAxisSize: MainAxisSize.min, children: [
        DropdownButton<String>(
          value: category,
          isExpanded: true,
          items: _expenseCategories
              .map((c) => DropdownMenuItem(value: c, child: Text(c)))
              .toList(),
          onChanged: (v) => setState(() => category = v ?? category),
        ),
        const SizedBox(height: 8),
        DropdownButton<String>(
          value: freq,
          isExpanded: true,
          items: _frequencies.map((f) => DropdownMenuItem(value: f, child: Text(f))).toList(),
          onChanged: (v) => setState(() => freq = v ?? freq),
        ),
      ]),
    );
    if (ok != true) return;
    final amount = double.tryParse(amt.text.trim()) ?? 0;
    if (amount <= 0) {
      _toast('Enter a positive amount');
      return;
    }
    setState(() => _expenses.add({
      'category': category,
      'description': desc.text.trim(),
      'amount': amount,
      'frequency': freq,
    }));
  }

  Future<void> _addAsset() async {
    final name = TextEditingController();
    final value = TextEditingController();
    String type = 'cash';
    final ok = await _formDialog(
      title: 'Add asset',
      fields: [
        (name, 'Name (e.g. savings account)', null),
        (value, 'Value', null),
      ],
      extra: (setState) => DropdownButton<String>(
        value: type,
        isExpanded: true,
        items: ['cash', 'investment', 'property', 'vehicle']
            .map((t) => DropdownMenuItem(value: t, child: Text(t)))
            .toList(),
        onChanged: (v) => setState(() => type = v ?? type),
      ),
    );
    if (ok != true) return;
    final v = double.tryParse(value.text.trim()) ?? 0;
    if (name.text.trim().isEmpty || v <= 0) {
      _toast('Enter a name and a positive value');
      return;
    }
    setState(() => _assets.add({'name': name.text.trim(), 'value': v, 'type': type}));
  }

  Future<void> _addLiability() async {
    final name = TextEditingController();
    final amount = TextEditingController();
    String type = 'loan';
    final ok = await _formDialog(
      title: 'Add liability',
      fields: [
        (name, 'Name (e.g. car loan)', null),
        (amount, 'Amount owed', null),
      ],
      extra: (setState) => DropdownButton<String>(
        value: type,
        isExpanded: true,
        items: ['loan', 'mortgage', 'credit_card']
            .map((t) => DropdownMenuItem(value: t, child: Text(t)))
            .toList(),
        onChanged: (v) => setState(() => type = v ?? type),
      ),
    );
    if (ok != true) return;
    final a = double.tryParse(amount.text.trim()) ?? 0;
    if (name.text.trim().isEmpty || a <= 0) {
      _toast('Enter a name and a positive amount');
      return;
    }
    setState(() => _liabilities.add({'name': name.text.trim(), 'amount': a, 'type': type}));
  }

  /// Generic two-text-field dialog with an optional inline widget
  /// (dropdowns). Returns true when confirmed.
  Future<bool?> _formDialog({
    required String title,
    required List<(TextEditingController, String, String?)> fields,
    required Widget Function(void Function(void Function())) extra,
  }) {
    return showDialog<bool>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialog) => AlertDialog(
          title: Text(title),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              ...fields.map(
                (f) => Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: TextField(
                    controller: f.$1,
                    decoration: InputDecoration(labelText: f.$2, hintText: f.$3),
                    keyboardType: f.$2.contains('Amount') ||
                            f.$2.contains('Value') ||
                            f.$2.contains('owed')
                        ? TextInputType.number
                        : null,
                  ),
                ),
              ),
              extra(setDialog),
            ],
          ),
          actions: [
            TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
            ElevatedButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Add')),
          ],
        ),
      ),
    );
  }

  @override
  void dispose() {
    _householdCtrl.dispose();
    _periodCtrl.dispose();
    super.dispose();
  }

  // -- build ------------------------------------------------------------

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Household finance')),
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Analyze your household\'s finances for a period.',
                style: theme.textTheme.bodySmall),
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(child: TextField(controller: _householdCtrl, decoration: const InputDecoration(labelText: 'Household ID', isDense: true))),
                const SizedBox(width: 12),
                Expanded(child: TextField(controller: _periodCtrl, decoration: const InputDecoration(labelText: 'Period (e.g. 2026-10)', isDense: true))),
              ],
            ),
            const SizedBox(height: 16),
            _section('Incomes', _incomes.length, Icons.trending_up, _addIncome,
                (i) => _incomes.removeAt(i),
                (m) => '${m['source']} - ${m['frequency']}'),
            _section('Expenses', _expenses.length, Icons.trending_down, _addExpense,
                (i) => _expenses.removeAt(i),
                (m) => '${m['category']} - ${m['frequency']}'),
            _section('Assets', _assets.length, Icons.account_balance, _addAsset,
                (i) => _assets.removeAt(i), (m) => '${m['type']}'),
            _section('Liabilities', _liabilities.length, Icons.credit_card, _addLiability,
                (i) => _liabilities.removeAt(i), (m) => '${m['type']}'),
            const SizedBox(height: 8),
            SizedBox(
              width: double.infinity,
              child: ElevatedButton.icon(
                onPressed: _busy ? null : _analyze,
                icon: _busy
                    ? const SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2))
                    : const Icon(Icons.analytics_outlined),
                label: const Text('Analyze household'),
              ),
            ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Text(_error!, style: TextStyle(color: theme.colorScheme.error)),
              ),
            if (_result != null) ...[
              const SizedBox(height: 16),
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('Result', style: theme.textTheme.titleMedium),
                      const SizedBox(height: 8),
                      _kv('Total income', _result!['total_income']),
                      _kv('Total expenses', _result!['total_expenses']),
                      _kv('Surplus / deficit', _result!['surplus_deficit']),
                      _kv('Savings rate', _result!['savings_rate']),
                      _kv('Net worth', _result!['net_worth']),
                      _kv('Budget health', _result!['budget_health']),
                      const Divider(height: 20),
                      if ((_result!['expense_by_category'] as Map).isNotEmpty)
                        Text('Expenses by category:', style: theme.textTheme.bodySmall),
                      ...(_result!['expense_by_category'] as Map).entries.map(
                            (e) => _kv('${e.key}', e.value),
                          ),
                    ],
                  ),
                ),
              ),
            ],
            const SizedBox(height: 24),
          ],
        ),
      ),
    );
  }

  Widget _kv(String k, dynamic v) {
    String s;
    if (v is num) {
      s = v is int ? v.toString() : v.toStringAsFixed(2);
    } else {
      s = '$v';
    }
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          Expanded(child: Text(k)),
          Text(s, style: const TextStyle(fontWeight: FontWeight.bold)),
        ],
      ),
    );
  }

  Widget _section(
    String title,
    int count,
    IconData icon,
    Future<void> Function() onAdd,
    void Function(int) onRemove,
    String Function(Map<String, dynamic>) subtitle,
  ) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(icon, size: 18),
            const SizedBox(width: 8),
            Text('$title ($count)', style: const TextStyle(fontWeight: FontWeight.bold)),
            const Spacer(),
            IconButton(icon: const Icon(Icons.add, size: 20), onPressed: onAdd),
          ],
        ),
        ...List.generate(count, (i) {
          final list = title == 'Incomes'
              ? _incomes
              : title == 'Expenses'
                  ? _expenses
                  : title == 'Assets'
                      ? _assets
                      : _liabilities;
          final m = list[i];
          return ListTile(
            dense: true,
            title: Text(
              '${m['name'] ?? m['source'] ?? m['description'] ?? ''} '
              '${m['amount'] ?? m['value'] ?? ''}',
            ),
            subtitle: Text(subtitle(m)),
            trailing: IconButton(
              icon: const Icon(Icons.delete_outline, size: 20),
              onPressed: () => setState(() => onRemove(i)),
            ),
          );
        }),
        const Divider(height: 16),
      ],
    );
  }
}
