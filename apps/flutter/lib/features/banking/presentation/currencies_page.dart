// The currencies this Book keeps, and the world's list it chooses from.
//
// A Book declares the money it deals in, and the engine answers how many decimal places each one's money has. This
// screen offers the whole catalogue rather than a typed code, because a code typed by hand is how a Book ends up
// rounding yen to cents.
import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/banking/data/currencies.dart';

class CurrenciesPage extends StatefulWidget {
  const CurrenciesPage({super.key, this.service, this.today});

  final CurrencyService? service;

  /// The day the engine seeds its defaults for. Fixed in a test so the page cannot drift with the clock.
  final String? today;

  @override
  State<CurrenciesPage> createState() => _CurrenciesPageState();
}

class _CurrenciesPageState extends State<CurrenciesPage> {
  late final CurrencyService _service =
      widget.service ?? CurrencyService.instance;

  CurrencyBook _book = CurrencyBook.none;
  List<IsoCurrency> _catalogue = const [];
  Object? _failure;
  bool _reading = true;
  bool _working = false;
  String? _said;

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
      final book = await _service.read();
      final catalogue = await _service.catalogue();
      if (!mounted) return;
      setState(() {
        _book = book;
        _catalogue = catalogue;
        _reading = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _reading = false;
        _failure = failure;
      });
    }
  }

  /// Asks the engine to add a currency, then reads back what the Book now keeps.
  Future<void> _add(IsoCurrency chosen) async {
    setState(() {
      _working = true;
      _said = null;
    });
    try {
      await _service.add(chosen.code, today: _today);
      final book = await _service.read();
      if (!mounted) return;
      setState(() {
        _book = book;
        _working = false;
        _said =
            '${chosen.name} added. Money in this Book can now be kept in it.';
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _working = false;
        _said = CouldNotRead.reasonOf(failure);
      });
    }
  }

  Future<void> _choose() async {
    final offered = [
      for (final one in _catalogue)
        if (!_book.holds(one.code)) one,
    ];
    final chosen = await showModalBottomSheet<IsoCurrency>(
      context: context,
      isScrollControlled: true,
      builder: (context) => _CurrencyChooser(offered: offered),
    );
    if (chosen != null) await _add(chosen);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('The currencies you keep')),
      body: _reading
          ? const Center(child: CircularProgressIndicator())
          : _failure != null
          ? CouldNotRead(
              what: 'the currencies',
              error: _failure,
              onRetry: _load,
            )
          : ListView(
              padding: const EdgeInsets.all(16),
              children: [
                Text(
                  'This Book keeps its money in these. Every amount is held exactly, to the '
                  'decimal places each currency actually has.',
                  style: Theme.of(context).textTheme.bodyMedium,
                ),
                const SizedBox(height: 12),
                if (_book.empty)
                  const Text(
                    'No currency yet. Add the money this Book deals in.',
                  ),
                for (final one in _book.held)
                  ListTile(
                    leading: const Icon(Icons.payments_outlined),
                    title: Text(one.code),
                    subtitle: Text(one.reading),
                  ),
                if (_said != null) ...[
                  const SizedBox(height: 12),
                  Text(_said!, style: Theme.of(context).textTheme.bodyMedium),
                ],
                const SizedBox(height: 16),
                FilledButton.icon(
                  onPressed: _working || _catalogue.isEmpty ? null : _choose,
                  icon: const Icon(Icons.add),
                  label: const Text('Add a currency'),
                ),
                if (_working)
                  const Padding(
                    padding: EdgeInsets.only(top: 12),
                    child: LinearProgressIndicator(),
                  ),
              ],
            ),
    );
  }
}

/// The world's currencies, searchable, so the owner picks from the list rather than typing a code.
class _CurrencyChooser extends StatefulWidget {
  const _CurrencyChooser({required this.offered});

  final List<IsoCurrency> offered;

  @override
  State<_CurrencyChooser> createState() => _CurrencyChooserState();
}

class _CurrencyChooserState extends State<_CurrencyChooser> {
  String _asked = '';

  @override
  Widget build(BuildContext context) {
    final wanted = _asked.trim().toLowerCase();
    final shown = [
      for (final one in widget.offered)
        if (wanted.isEmpty ||
            one.code.toLowerCase().contains(wanted) ||
            one.name.toLowerCase().contains(wanted))
          one,
    ];
    return SafeArea(
      child: Padding(
        padding: EdgeInsets.only(
          bottom: MediaQuery.of(context).viewInsets.bottom,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
              child: TextField(
                decoration: const InputDecoration(
                  labelText: 'Which currency',
                  hintText: 'USD, or dollar',
                ),
                onChanged: (value) => setState(() => _asked = value),
              ),
            ),
            Flexible(
              child: ListView.builder(
                shrinkWrap: true,
                itemCount: shown.length,
                itemBuilder: (context, index) {
                  final one = shown[index];
                  return ListTile(
                    title: Text('${one.code} · ${one.name}'),
                    subtitle: Text(one.placesWord),
                    onTap: () => Navigator.of(context).pop(one),
                  );
                },
              ),
            ),
          ],
        ),
      ),
    );
  }
}
