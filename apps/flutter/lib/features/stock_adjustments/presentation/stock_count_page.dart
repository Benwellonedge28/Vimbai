// What a count found the stock to be.
//
// A business counts its stock and finds a different figure from the one its books carry. That difference is money: stock
// was paid for and sits in the accounts as something owned, so a count that comes up short means part of what the books
// claim to own is gone, and a count that comes up over means the business owns more than it thought.
//
// The screen asks which way the count went, what was counted and what the difference comes to, and it says what each way
// does to the books before it is recorded: a shortage charges the month and brings stock down, found stock puts it back and
// takes the earlier charge back. It never multiplies a quantity by a price, because the engine does not know the business's
// costing method and a guessed figure is one nobody can defend. It says plainly that nothing moves money: counting stock
// never pays anybody or collects anything.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/stock_adjustments/data/stock_adjustments_posting.dart';

class StockCountPage extends StatefulWidget {
  const StockCountPage({super.key, this.posting});

  /// A test passes its own service so the page can be exercised without a server.
  final StockAdjustmentsPostingService? posting;

  @override
  State<StockCountPage> createState() => _StockCountPageState();
}

class _StockCountPageState extends State<StockCountPage> {
  late final StockAdjustmentsPostingService _posting =
      widget.posting ?? StockAdjustmentsPostingService.instance;

  final TextEditingController _day = TextEditingController(text: _today());
  final TextEditingController _item = TextEditingController();
  final TextEditingController _figure = TextEditingController();
  final TextEditingController _reason = TextEditingController();

  /// `down` when the books carry more than there is, `up` when there is more than the books carry.
  String _direction = 'down';

  List<StockAdjustment> _found = const [];
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
    _day.dispose();
    _item.dispose();
    _figure.dispose();
    _reason.dispose();
    super.dispose();
  }

  static String _today() {
    final now = DateTime.now();
    return '${now.year}-${now.month.toString().padLeft(2, '0')}-'
        '${now.day.toString().padLeft(2, '0')}';
  }

  Future<void> _read() async {
    setState(() {
      _reading = true;
      _trouble = null;
    });
    try {
      final found = await _posting.read();
      if (!mounted) return;
      setState(() {
        _found = found;
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

  bool get _short => _direction == 'down';

  /// Everything that would stop the engine posting, said before it is asked.
  String? get _refusal {
    if (!RegExp(r'^\d{4}-\d{2}-\d{2}$').hasMatch(_day.text.trim())) {
      return 'Say the day the stock was counted, as 2026-11-20.';
    }
    if (_item.text.trim().isEmpty) {
      return 'Say what was counted: the item, or the reference the business knows it by.';
    }
    final typed = _figure.text.trim();
    if (typed.isEmpty) {
      return 'Say what the difference comes to in money, as 180.00.';
    }
    final figure = double.tryParse(typed);
    if (figure == null) {
      return 'That is not a figure the books can carry. Say it as 180.00.';
    }
    if (figure <= 0) {
      return 'A difference has to be more than nothing: say what it comes to.';
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
      final recorded = await _posting.post(
        entryDate: _day.text.trim(),
        item: _item.text.trim(),
        direction: _direction,
        amount: _figure.text.trim(),
        reason: _reason.text,
      );
      if (!mounted) return;
      setState(() {
        _found = [recorded, ..._found];
        _saving = false;
        _item.clear();
        _figure.clear();
        _reason.clear();
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _trouble = CouldNotRead.reasonOf(failure);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Stock that is not what the books say')),
      body: _reading
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
              children: [
                Text(
                  'A count that comes up short means part of what the books claim to own is gone: it was paid for, so the '
                  'month carries the cost of it. A count that comes up over puts it back.',
                  style: theme.textTheme.bodyMedium,
                ),
                const SizedBox(height: 20),
                SegmentedButton<String>(
                  segments: const [
                    ButtonSegment(
                      value: 'down',
                      label: Text('Less than the books'),
                    ),
                    ButtonSegment(
                      value: 'up',
                      label: Text('More than the books'),
                    ),
                  ],
                  selected: {_direction},
                  onSelectionChanged: (chosen) => setState(() {
                    _direction = chosen.first;
                    _trouble = null;
                  }),
                ),
                const SizedBox(height: 8),
                Text(
                  _short
                      ? 'Stock damaged, stolen, spoiled or gone out of date. The difference is charged to the month and '
                            'stock comes down. No supplier owes anything, and nothing was sold.'
                      : 'Stock found that the books did not carry, usually something written down too hard or a delivery '
                            'never recorded. Stock goes back up and the cost carried earlier comes back with it.',
                  style: theme.textTheme.bodySmall,
                ),
                const SizedBox(height: 20),
                TextField(
                  controller: _day,
                  decoration: const InputDecoration(
                    labelText: 'The day it was counted',
                    helperText: 'As 2026-11-20: the month follows from it.',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) => setState(() {}),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _item,
                  decoration: const InputDecoration(
                    labelText: 'What was counted',
                    helperText:
                        'The item, or the reference the business knows it by.',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) => setState(() {}),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _figure,
                  keyboardType: const TextInputType.numberWithOptions(
                    decimal: true,
                  ),
                  decoration: const InputDecoration(
                    labelText: 'What the difference comes to',
                    helperText:
                        'In money, as 180.00: what the count says, not a quantity times a price.',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) => setState(() {}),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _reason,
                  decoration: const InputDecoration(
                    labelText: 'Why',
                    helperText:
                        'Optional: water damage, a recount, a delivery never recorded.',
                    border: OutlineInputBorder(),
                  ),
                ),
                const SizedBox(height: 20),
                FilledButton(
                  onPressed: _saving ? null : _record,
                  child: Text(
                    _saving ? 'Recording...' : 'Record what the count found',
                  ),
                ),
                const SizedBox(height: 8),
                Text(
                  'Nothing moves: counting stock never pays anybody or collects anything.',
                  style: theme.textTheme.bodySmall,
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
                const SizedBox(height: 24),
                Text(
                  'What counting has found',
                  style: theme.textTheme.titleSmall,
                ),
                const SizedBox(height: 6),
                if (_found.isEmpty)
                  Text(
                    'Nothing yet: this Book has not recorded a count.',
                    style: theme.textTheme.bodySmall,
                  ),
                for (final one in _found)
                  ListTile(
                    contentPadding: EdgeInsets.zero,
                    dense: true,
                    title: Text(one.item),
                    subtitle: Text(
                      '${one.entryDate} · ${one.way}'
                      '${one.reason.trim().isEmpty ? '' : ' · ${one.reason}'}',
                    ),
                    trailing: Text(one.amount),
                  ),
              ],
            ),
    );
  }
}
