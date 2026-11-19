// The discounts this Book gives: what comes off, off what, for whom, and when it holds.
//
// This is the screen where a discount is declared, so the set of discounts is never a list the app was built with. Each
// row says what it takes off and who it is for, and a rule that cannot be given says what it is missing instead of
// looking finished.
//
// After a discount is saved the screen asks the engine what would come off a thousand and shows that answer. The app
// never works out a percentage itself, so what is shown here and what the journal posts are the same figure.
import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/discounts.dart';

/// What the form collected, before the engine has seen it.
class DiscountDraft {
  const DiscountDraft({
    required this.name,
    required this.kind,
    required this.value,
    required this.compulsory,
    required this.allCustomers,
    required this.customerIds,
    required this.minTotal,
    required this.from,
    required this.until,
  });

  final String name;
  final String kind;
  final String value;
  final bool compulsory;
  final bool allCustomers;
  final List<String> customerIds;
  final String minTotal;
  final String from;
  final String until;
}

class DiscountsPage extends StatefulWidget {
  const DiscountsPage({super.key, this.service, this.today});

  final DiscountService? service;

  /// The day to read the rules for. Fixed in a test so the page cannot drift with the clock.
  final String? today;

  @override
  State<DiscountsPage> createState() => _DiscountsPageState();
}

class _DiscountsPageState extends State<DiscountsPage> {
  late final DiscountService _service =
      widget.service ?? DiscountService.instance;

  DiscountTable? _table;
  Object? _failure;
  bool _reading = true;
  bool _working = false;

  /// What the engine said would come off a thousand, once a discount has been saved.
  String? _proof;

  @override
  void initState() {
    super.initState();
    _load();
  }

  String get _today {
    final fixed = widget.today;
    if (fixed != null && fixed.isNotEmpty) return fixed;
    final now = DateTime.now();
    final month = now.month.toString().padLeft(2, '0');
    final day = now.day.toString().padLeft(2, '0');
    return '${now.year}-$month-$day';
  }

  Future<void> _load() async {
    setState(() {
      _reading = true;
      _failure = null;
    });
    try {
      final table = await _service.read();
      if (!mounted) return;
      setState(() {
        _table = table;
        _reading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _failure = error;
        _reading = false;
      });
    }
  }

  /// Declare a discount, then ask the engine what it takes off so the answer is shown before it is used.
  Future<void> _add(DiscountDraft draft) async {
    setState(() {
      _working = true;
      _proof = null;
    });
    try {
      final saved = await _service.save(
        name: draft.name,
        kind: draft.kind,
        value: draft.value,
        compulsory: draft.compulsory,
        allCustomers: draft.allCustomers,
        customerIds: draft.customerIds,
        minTotal: draft.minTotal.isEmpty ? '0' : draft.minTotal,
        effectiveFrom: draft.from,
        effectiveTo: draft.until,
      );
      final table = await _service.read();
      final quote = await _service.quote(
        total: '1000.00',
        customerId: draft.allCustomers
            ? 'anyone'
            : (draft.customerIds.isEmpty ? 'anyone' : draft.customerIds.first),
        date: _today,
        rules: draft.compulsory ? const [] : [saved.name],
      );
      if (!mounted) return;
      setState(() {
        _table = table;
        _proof = quote.reading;
        _working = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _failure = null;
        _working = false;
      });
      // the engine's refusal is shown in its own words: it names the customer or the day to fix
      await showDialog<void>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('This discount was not saved'),
          content: Text(CouldNotRead.reasonOf(error)),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('Right'),
            ),
          ],
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('The discounts you give')),
      body: _body(context),
      floatingActionButton: _reading || _failure != null
          ? null
          : FloatingActionButton.extended(
              onPressed: _working ? null : () => _openForm(context),
              icon: const Icon(Icons.add),
              label: const Text('Add a discount'),
            ),
    );
  }

  Widget _body(BuildContext context) {
    if (_reading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_failure != null) {
      return CouldNotRead(
        what: 'The discounts this Book gives',
        error: _failure,
        onRetry: _load,
      );
    }
    final table = _table;
    if (table == null) {
      return const SizedBox.shrink();
    }
    final theme = Theme.of(context);
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 96),
        children: [
          Text(table.headline, style: theme.textTheme.titleLarge),
          const SizedBox(height: 6),
          Text(table.detail, style: theme.textTheme.bodyMedium),
          if (_proof != null) ...[
            const SizedBox(height: 14),
            Card(
              color: theme.colorScheme.surfaceContainerHighest,
              child: Padding(
                padding: const EdgeInsets.all(14),
                child: Text(_proof!, style: theme.textTheme.bodyMedium),
              ),
            ),
          ],
          if (table.empty) ...[
            const SizedBox(height: 20),
            Text(
              'A discount is a percentage or a sum of money. It can be given on every invoice it fits, or offered and '
              'taken only when somebody asks for it, and it can be for every customer or only the ones you name.',
              style: theme.textTheme.bodySmall,
            ),
          ],
          const SizedBox(height: 18),
          for (final discount in table.discounts)
            _DiscountRow(discount: discount),
          if (table.discounts.isNotEmpty) ...[
            const SizedBox(height: 20),
            Text(
              'A discount comes off before tax, so the customer is taxed on what they pay. An invoice keeps the '
              'discount it was raised with, even if the rule changes later.',
              style: theme.textTheme.bodySmall,
            ),
          ],
        ],
      ),
    );
  }

  Future<void> _openForm(BuildContext context) async {
    final draft = await showModalBottomSheet<DiscountDraft>(
      context: context,
      isScrollControlled: true,
      builder: (context) => Padding(
        padding: EdgeInsets.only(
          bottom: MediaQuery.of(context).viewInsets.bottom,
        ),
        child: _DiscountForm(
          today: _today,
          existing: [
            for (final one in _table?.discounts ?? const <BookDiscount>[])
              one.name,
          ],
        ),
      ),
    );
    if (draft == null) return;
    await _add(draft);
  }
}

class _DiscountRow extends StatelessWidget {
  const _DiscountRow({required this.discount});

  final BookDiscount discount;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final problem = discount.problem;
    return Card(
      margin: const EdgeInsets.only(bottom: 10),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    discount.name,
                    style: theme.textTheme.titleMedium,
                  ),
                ),
                Text(discount.valueText, style: theme.textTheme.titleMedium),
              ],
            ),
            const SizedBox(height: 4),
            Text(discount.reading, style: theme.textTheme.bodyMedium),
            if (problem.isNotEmpty) ...[
              const SizedBox(height: 6),
              Text(
                problem,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.error,
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _DiscountForm extends StatefulWidget {
  const _DiscountForm({required this.today, required this.existing});

  final String today;
  final List<String> existing;

  @override
  State<_DiscountForm> createState() => _DiscountFormState();
}

class _DiscountFormState extends State<_DiscountForm> {
  final _name = TextEditingController();
  final _value = TextEditingController();
  final _customers = TextEditingController();
  final _minTotal = TextEditingController();
  late final _from = TextEditingController(text: widget.today);

  /// A year on, because a discount is given for a season rather than forever. It is a default to change, not a rule.
  late final _until = TextEditingController(text: _aYearAfter(widget.today));

  static String _aYearAfter(String day) {
    final parts = day.split('-');
    if (parts.length != 3) return '';
    final year = int.tryParse(parts[0]);
    if (year == null) return '';
    return '${year + 1}-${parts[1]}-${parts[2]}';
  }

  String _kind = 'percent';
  bool _compulsory = true;
  bool _allCustomers = true;
  String? _trouble;

  @override
  void dispose() {
    _name.dispose();
    _value.dispose();
    _customers.dispose();
    _minTotal.dispose();
    _from.dispose();
    _until.dispose();
    super.dispose();
  }

  void _submit() {
    final name = _name.text.trim();
    if (name.isEmpty) {
      setState(() => _trouble = 'Give the discount a name.');
      return;
    }
    if (widget.existing.contains(name)) {
      setState(
        () => _trouble =
            'This Book already has a discount called $name. Give this one a different name.',
      );
      return;
    }
    final value = _value.text.trim();
    if (value.isEmpty) {
      setState(
        () => _trouble = _kind == 'percent'
            ? 'Say the percentage it takes off, like 10.'
            : 'Say the sum of money it takes off, like 250.00.',
      );
      return;
    }
    if (_from.text.trim().isEmpty) {
      setState(
        () => _trouble = 'Say the day it starts applying, like 2026-01-01.',
      );
      return;
    }
    if (_until.text.trim().isEmpty) {
      setState(
        () => _trouble =
            'Say the day it stops applying, like 2026-12-31. Every discount runs from a day to a day.',
      );
      return;
    }
    final ids = [
      for (final one in _customers.text.split(','))
        if (one.trim().isNotEmpty) one.trim(),
    ];
    if (!_allCustomers && ids.isEmpty) {
      setState(
        () => _trouble =
            'Name the customers this discount is for, or make it for every customer.',
      );
      return;
    }
    Navigator.of(context).pop(
      DiscountDraft(
        name: name,
        kind: _kind,
        value: value,
        compulsory: _compulsory,
        allCustomers: _allCustomers,
        customerIds: ids,
        minTotal: _minTotal.text.trim(),
        from: _from.text.trim(),
        until: _until.text.trim(),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 16),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('A new discount', style: theme.textTheme.titleLarge),
            const SizedBox(height: 12),
            TextField(
              controller: _name,
              decoration: const InputDecoration(
                labelText: 'Name',
                hintText: 'Trade terms',
              ),
            ),
            const SizedBox(height: 12),
            SegmentedButton<String>(
              segments: const [
                ButtonSegment(value: 'percent', label: Text('A percentage')),
                ButtonSegment(value: 'amount', label: Text('A sum of money')),
              ],
              selected: {_kind},
              onSelectionChanged: (chosen) =>
                  setState(() => _kind = chosen.first),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _value,
              keyboardType: const TextInputType.numberWithOptions(
                decimal: true,
              ),
              decoration: InputDecoration(
                labelText: _kind == 'percent' ? 'Percentage' : 'Amount',
                hintText: _kind == 'percent' ? '10' : '250.00',
              ),
            ),
            const SizedBox(height: 6),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: _compulsory,
              onChanged: (on) => setState(() => _compulsory = on),
              title: const Text('Given automatically'),
              subtitle: Text(
                _compulsory
                    ? 'Comes off every invoice it fits, without anyone asking.'
                    : 'Offered on an invoice, and taken only when somebody asks for it.',
              ),
            ),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: _allCustomers,
              onChanged: (on) => setState(() => _allCustomers = on),
              title: const Text('For every customer'),
              subtitle: Text(
                _allCustomers
                    ? 'Any customer who is invoiced takes it.'
                    : 'Only the customers named below take it.',
              ),
            ),
            if (!_allCustomers) ...[
              const SizedBox(height: 4),
              TextField(
                controller: _customers,
                decoration: const InputDecoration(
                  labelText: 'Customers',
                  hintText: 'ACME-1, OTHER-2',
                ),
              ),
            ],
            const SizedBox(height: 12),
            TextField(
              controller: _minTotal,
              keyboardType: const TextInputType.numberWithOptions(
                decimal: true,
              ),
              decoration: const InputDecoration(
                labelText: 'Only once an invoice reaches',
                hintText: 'leave empty for every invoice',
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _from,
              decoration: const InputDecoration(
                labelText: 'Starts applying',
                hintText: '2026-01-01',
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _until,
              decoration: const InputDecoration(
                labelText: 'Stops applying',
                hintText: '2026-12-31',
                helperText: 'Every discount runs from a day to a day.',
              ),
            ),
            if (_trouble != null) ...[
              const SizedBox(height: 10),
              Text(
                _trouble!,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.error,
                ),
              ),
            ],
            const SizedBox(height: 16),
            Row(
              children: [
                Expanded(
                  child: Text(
                    'The engine decides the figure, and says why not when it refuses.',
                    style: theme.textTheme.bodySmall,
                  ),
                ),
                const SizedBox(width: 12),
                FilledButton(onPressed: _submit, child: const Text('Add it')),
              ],
            ),
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
  }
}
