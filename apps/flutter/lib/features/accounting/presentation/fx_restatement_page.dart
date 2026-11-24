// Restating the month: the balances held in another money, the closing rate, and what the rate did.
//
// The one thing this screen must not do is invent the rate. What a currency is worth at a period end is a fact about the
// world, so the person gives it, per currency, and the engine does everything else: it reads the balances, works out what
// the rate did, posts it as an unrealised difference in the period it belongs to, and refuses a month that has been closed.
//
// Vimbai moves no money here. A restatement is a record of what the books already say, restated.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/fx_restatement.dart';

class FxRestatementPage extends StatefulWidget {
  const FxRestatementPage({super.key, this.service, this.entryDate});

  /// A test passes its own service so the page can be exercised without a server.
  final FxRestatementService? service;

  /// The day the restatement is dated on. Defaults to the last day of the month just ended.
  final DateTime? entryDate;

  @override
  State<FxRestatementPage> createState() => _FxRestatementPageState();
}

class _FxRestatementPageState extends State<FxRestatementPage> {
  late final FxRestatementService _service =
      widget.service ?? FxRestatementService.instance;

  late DateTime _entryDate = widget.entryDate ?? _endOfLastMonth();

  final Map<String, TextEditingController> _rates = {};
  final Map<String, bool> _included = {};
  final List<String> _said = [];

  bool _loading = true;
  bool _posting = false;
  String? _error;
  ForeignHoldings? _holdings;

  static DateTime _endOfLastMonth() {
    final today = DateTime.now();
    return DateTime(today.year, today.month, 0);
  }

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    for (final rate in _rates.values) {
      rate.dispose();
    }
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final holdings = await _service.holdings();
      if (!mounted) return;
      for (final holding in holdings.currencies) {
        _rates.putIfAbsent(holding.currency, TextEditingController.new);
        _included.putIfAbsent(holding.currency, () => true);
      }
      setState(() {
        _holdings = holdings;
        _loading = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _error = CouldNotRead.reasonOf(failure);
        _loading = false;
      });
    }
  }

  Future<void> _pickDate() async {
    final picked = await showDatePicker(
      context: context,
      firstDate: DateTime(2000),
      lastDate: DateTime(2101),
      initialDate: _entryDate,
    );
    if (picked == null || !mounted) return;
    setState(() => _entryDate = picked);
  }

  /// The rate typed for a currency, or nothing when it does not read as a figure.
  ExactAmount? _rateFor(String currency) {
    final typed = (_rates[currency]?.text ?? '').trim();
    if (typed.isEmpty) return null;
    try {
      final rate = ExactAmount.parse(typed);
      return rate.isZero ? null : rate;
    } catch (_) {
      return null;
    }
  }

  Future<void> _restate() async {
    final holdings = _holdings;
    if (holdings == null) return;
    final ready = [
      for (final holding in holdings.currencies)
        if ((_included[holding.currency] ?? false) &&
            _rateFor(holding.currency) != null)
          holding,
    ];
    if (ready.isEmpty) {
      setState(
        () => _said.add(
          'Give a closing rate for at least one currency, and it will be restated.',
        ),
      );
      return;
    }
    setState(() {
      _posting = true;
      _said.clear();
    });
    final day =
        '${_entryDate.year.toString().padLeft(4, '0')}-'
        '${_entryDate.month.toString().padLeft(2, '0')}-'
        '${_entryDate.day.toString().padLeft(2, '0')}';
    for (final holding in ready) {
      final rate = _rateFor(holding.currency)!;
      try {
        final posted = await _service.restate(
          entryDate: day,
          closingRate: rate.toDecimalString(),
          balances: [
            for (final document in holding.documents)
              RestatementBalance(
                account: document.account,
                units: document.signedUnits,
                carried: document.signedCarried,
              ),
          ],
        );
        if (!mounted) return;
        setState(
          () => _said.add(
            posted.isQuiet
                ? '${holding.currency} was already at ${rate.toDecimalString()}, so nothing changed.'
                : '${holding.currency}: gained ${posted.gain.toDecimalString()}, '
                      'lost ${posted.loss.toDecimalString()} over ${posted.period}.',
          ),
        );
      } catch (failure) {
        if (!mounted) return;
        setState(() => _said.add(CouldNotRead.reasonOf(failure)));
      }
    }
    if (!mounted) return;
    setState(() => _posting = false);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Restate the month'),
        actions: [
          IconButton(
            tooltip: 'The day it is dated on',
            onPressed: _pickDate,
            icon: const Icon(Icons.event),
          ),
        ],
      ),
      body: _body(),
      bottomNavigationBar: _holdings == null || _holdings!.isEmpty
          ? null
          : SafeArea(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: ElevatedButton(
                  onPressed: _posting ? null : _restate,
                  child: Text(
                    _posting ? 'Restating' : 'Restate at ${_day(_entryDate)}',
                  ),
                ),
              ),
            ),
    );
  }

  static String _day(DateTime date) =>
      '${date.year.toString().padLeft(4, '0')}-'
      '${date.month.toString().padLeft(2, '0')}-'
      '${date.day.toString().padLeft(2, '0')}';

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'What is held in another money',
        error: _error!,
        onRetry: _load,
      );
    }
    final holdings = _holdings;
    if (holdings == null) {
      return CouldNotRead(
        what: 'What is held in another money',
        error: 'The reading came back empty.',
        onRetry: _load,
      );
    }
    final theme = Theme.of(context);
    if (holdings.isEmpty) {
      return Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Nothing is held in another money',
              style: theme.textTheme.titleLarge,
            ),
            const SizedBox(height: 8),
            Text(
              'No sale or bill has been raised in a currency other than this '
              'Book\'s own, so there is nothing to restate.',
              style: theme.textTheme.bodyMedium,
            ),
          ],
        ),
      );
    }
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Text(
          'Give what each money is worth on ${_day(_entryDate)}. The engine '
          'restates the balances at that rate and posts the difference as an '
          'unrealised one, in the month it belongs to.',
          style: theme.textTheme.bodySmall,
        ),
        if (holdings.note.isNotEmpty) ...[
          const SizedBox(height: 8),
          Text(holdings.note, style: theme.textTheme.bodySmall),
        ],
        const SizedBox(height: 12),
        for (final holding in holdings.currencies)
          _HoldingCard(
            holding: holding,
            rate: _rates[holding.currency]!,
            included: _included[holding.currency] ?? true,
            onInclude: (on) => setState(() => _included[holding.currency] = on),
            onRateChanged: () => setState(() {}),
          ),
        if (_said.isNotEmpty) ...[
          const SizedBox(height: 16),
          for (final words in _said)
            Padding(
              padding: const EdgeInsets.only(bottom: 6),
              child: Text(words, style: theme.textTheme.bodyMedium),
            ),
        ],
      ],
    );
  }
}

class _HoldingCard extends StatelessWidget {
  const _HoldingCard({
    required this.holding,
    required this.rate,
    required this.included,
    required this.onInclude,
    required this.onRateChanged,
  });

  final CurrencyHolding holding;
  final TextEditingController rate;
  final bool included;
  final ValueChanged<bool> onInclude;
  final VoidCallback onRateChanged;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    ExactAmount? closing;
    final typed = rate.text.trim();
    if (typed.isNotEmpty) {
      try {
        closing = ExactAmount.parse(typed);
        if (closing.isZero) closing = null;
      } catch (_) {
        closing = null;
      }
    }
    final difference = closing == null ? null : holding.differenceAt(closing);
    final gained =
        difference != null && !difference.isNegative && !difference.isZero;

    return Card(
      margin: const EdgeInsets.only(bottom: 12),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    '${holding.currency} · ${holding.documents.length} '
                    '${holding.documents.length == 1 ? 'document' : 'documents'}',
                    style: theme.textTheme.titleMedium,
                  ),
                ),
                Switch(value: included, onChanged: onInclude),
              ],
            ),
            Text(
              '${holding.units.toDecimalString()} ${holding.currency}, carried '
              'at ${holding.carried.toDecimalString()}.',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 8),
            TextField(
              controller: rate,
              enabled: included,
              keyboardType: const TextInputType.numberWithOptions(
                decimal: true,
              ),
              decoration: InputDecoration(
                labelText: 'What one ${holding.currency} is worth',
                helperText: 'On the day being restated.',
              ),
              onChanged: (_) => onRateChanged(),
            ),
            if (difference != null) ...[
              const SizedBox(height: 8),
              Text(
                difference.isZero
                    ? 'At that rate nothing changes.'
                    : '${gained ? 'Would gain' : 'Would lose'} '
                          '${(gained ? difference : difference.negated()).toDecimalString()}',
                style: theme.textTheme.bodyMedium,
              ),
            ],
          ],
        ),
      ),
    );
  }
}
