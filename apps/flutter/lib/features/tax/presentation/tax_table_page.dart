// The taxes this Book charges: what each one takes, where it goes, and when it holds.
//
// This is the screen where a tax type is added, so the set of taxes is never a list the app was built with. Each row
// says what it charges and on what, and the row that cannot be charged says what it is missing instead of looking
// finished.
//
// After a tax is saved the screen asks the engine what a thousand would carry and shows that answer. The app never
// works out a percentage itself, so what is shown here and what the journal posts are the same figure.
import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/tax/data/tax_table.dart';

class TaxTablePage extends StatefulWidget {
  const TaxTablePage({super.key, this.service, this.today});

  final TaxTableService? service;

  /// The day to read the table for. Fixed in a test so the page cannot drift with the clock.
  final String? today;

  @override
  State<TaxTablePage> createState() => _TaxTablePageState();
}

class _TaxTablePageState extends State<TaxTablePage> {
  late final TaxTableService _service =
      widget.service ?? TaxTableService.instance;

  TaxTable? _table;
  Object? _failure;
  bool _reading = true;
  bool _working = false;

  /// What the engine said a thousand would carry, once a tax has been saved.
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
      final table = await _service.read(date: _today);
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

  /// The usual taxes, posted to the accounts this Book already has.
  Future<void> _seed() async {
    if (_working) return;
    setState(() {
      _working = true;
      _proof = null;
    });
    try {
      final table = await _service.seed();
      if (!mounted) return;
      setState(() {
        _table = table;
        _working = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _failure = error;
        _working = false;
      });
    }
  }

  /// Add a tax, then ask the engine what it charges so the answer is shown before it is used.
  Future<void> _add(BookTaxDraft draft) async {
    setState(() {
      _working = true;
      _proof = null;
    });
    try {
      final saved = await _service.save(
        name: draft.name,
        rate: draft.rate,
        scope: draft.scope,
        collectedAccount: draft.account,
        reclaimAccount: draft.account,
        effectiveFrom: draft.from,
        appliesAfter: draft.appliesAfter,
      );
      final table = await _service.read(date: _today);
      final preview = saved.onSales
          ? await _service.preview(
              date: _today,
              direction: 'sale',
              gross: '1000.00',
              taxes: [saved.name],
            )
          : await _service.preview(
              date: _today,
              direction: 'purchase',
              gross: '1000.00',
              taxes: [saved.name],
            );
      if (!mounted) return;
      setState(() {
        _table = table;
        _proof = preview.reading;
        _working = false;
      });
    } catch (error) {
      if (!mounted) return;
      // the engine's refusal is shown in its own words: it names the account or the date to fix
      setState(() {
        _failure = null;
        _working = false;
      });
      if (mounted) {
        await showDialog<void>(
          context: context,
          builder: (context) => AlertDialog(
            title: const Text('This tax was not saved'),
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
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('The taxes you charge')),
      body: _body(context),
      floatingActionButton: _reading || _failure != null
          ? null
          : FloatingActionButton.extended(
              onPressed: _working ? null : () => _openForm(context),
              icon: const Icon(Icons.add),
              label: const Text('Add a tax'),
            ),
    );
  }

  Widget _body(BuildContext context) {
    if (_reading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_failure != null) {
      return CouldNotRead(
        what: 'The taxes this Book charges',
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
            FilledButton.icon(
              onPressed: _working ? null : _seed,
              icon: const Icon(Icons.playlist_add_check),
              label: const Text('Fill in the usual taxes'),
            ),
            const SizedBox(height: 8),
            Text(
              'VAT, withholding on contracts, and named zero-rated and exempt entries, on the accounts this Book '
              'already has. Nothing is overwritten, and no rate changes by itself later.',
              style: theme.textTheme.bodySmall,
            ),
          ],
          const SizedBox(height: 18),
          for (final tax in table.taxes) _TaxRow(tax: tax),
          if (table.taxes.isNotEmpty) ...[
            const SizedBox(height: 20),
            Text(
              'A document carries no tax until one is chosen, and a tax posted under a rate keeps that rate when the '
              'rate changes.',
              style: theme.textTheme.bodySmall,
            ),
          ],
        ],
      ),
    );
  }

  Future<void> _openForm(BuildContext context) async {
    final table = _table;
    final draft = await showModalBottomSheet<BookTaxDraft>(
      context: context,
      isScrollControlled: true,
      builder: (context) => Padding(
        padding: EdgeInsets.only(
          bottom: MediaQuery.of(context).viewInsets.bottom,
        ),
        child: _TaxForm(
          today: _today,
          accounts: [
            for (final tax in (table?.taxes ?? const <BookTax>[]))
              if (tax.collectedAccount.isNotEmpty) tax.collectedAccount,
          ],
          existing: [
            for (final tax in (table?.taxes ?? const <BookTax>[])) tax.name,
          ],
        ),
      ),
    );
    if (draft == null || !mounted) return;
    await _add(draft);
  }
}

/// One tax, said in a sentence, with what is wrong with it when something is.
class _TaxRow extends StatelessWidget {
  const _TaxRow({required this.tax});

  final BookTax tax;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final problem = tax.problem;
    return Card(
      margin: const EdgeInsets.only(bottom: 10),
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(tax.name, style: theme.textTheme.titleMedium),
                ),
                Text(tax.rateText, style: theme.textTheme.titleMedium),
              ],
            ),
            const SizedBox(height: 4),
            Text(tax.reading, style: theme.textTheme.bodyMedium),
            if (problem.isNotEmpty) ...[
              const SizedBox(height: 6),
              Text(
                problem,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.error,
                ),
              ),
            ] else if (!tax.inForce) ...[
              const SizedBox(height: 6),
              Text(
                'Not in force on this day.',
                style: theme.textTheme.bodySmall,
              ),
            ],
          ],
        ),
      ),
    );
  }
}

/// What the form collected, before the engine has seen it.
class BookTaxDraft {
  const BookTaxDraft({
    required this.name,
    required this.rate,
    required this.scope,
    required this.account,
    required this.from,
    required this.appliesAfter,
  });

  final String name;
  final String rate;
  final String scope;
  final String account;
  final String from;
  final List<String> appliesAfter;
}

/// The form for a new tax: what it is called, what it takes, where it posts, and when it starts.
class _TaxForm extends StatefulWidget {
  const _TaxForm({
    required this.today,
    required this.accounts,
    required this.existing,
  });

  final String today;

  /// Account numbers already used by taxes in this Book, offered as the likely answer.
  final List<String> accounts;

  /// Taxes already defined, which this one may be charged after.
  final List<String> existing;

  @override
  State<_TaxForm> createState() => _TaxFormState();
}

class _TaxFormState extends State<_TaxForm> {
  late final TextEditingController _name = TextEditingController();
  late final TextEditingController _rate = TextEditingController();
  late final TextEditingController _account = TextEditingController(
    text: widget.accounts.isEmpty ? '' : widget.accounts.first,
  );
  String _scope = 'both';
  final Set<String> _after = {};

  @override
  void dispose() {
    _name.dispose();
    _rate.dispose();
    _account.dispose();
    super.dispose();
  }

  /// What stops this being saved, said the way the fix would be, or nothing when it can be saved.
  String get _blocked {
    if (_name.text.trim().isEmpty) return 'Give the tax a name, like VAT.';
    if (_rate.text.trim().isEmpty) return 'Give the tax a percentage.';
    if (_account.text.trim().isEmpty) {
      return 'Say which account it posts to, or it cannot be charged.';
    }
    return '';
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(20, 20, 20, 20),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('A tax to charge', style: theme.textTheme.titleLarge),
          const SizedBox(height: 14),
          TextField(
            controller: _name,
            decoration: const InputDecoration(
              labelText: 'What it is called',
              hintText: 'VAT',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 10),
          TextField(
            controller: _rate,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'Percentage',
              hintText: '15.5',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 14),
          Text('Charged on', style: theme.textTheme.labelLarge),
          const SizedBox(height: 6),
          SegmentedButton<String>(
            segments: const [
              ButtonSegment(value: 'sales', label: Text('Sales')),
              ButtonSegment(value: 'purchases', label: Text('Purchases')),
              ButtonSegment(value: 'both', label: Text('Both')),
            ],
            selected: {_scope},
            onSelectionChanged: (choice) =>
                setState(() => _scope = choice.first),
          ),
          const SizedBox(height: 14),
          TextField(
            controller: _account,
            keyboardType: TextInputType.number,
            decoration: const InputDecoration(
              labelText: 'Account it posts to',
              hintText: '2100',
            ),
            onChanged: (_) => setState(() {}),
          ),
          if (widget.existing.isNotEmpty) ...[
            const SizedBox(height: 14),
            Text('Charged after', style: theme.textTheme.labelLarge),
            const SizedBox(height: 4),
            Text(
              'Leave this alone unless the percentage should be worked out on top of another tax.',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 6),
            Wrap(
              spacing: 8,
              children: [
                for (final name in widget.existing)
                  FilterChip(
                    label: Text(name),
                    selected: _after.contains(name),
                    onSelected: (on) => setState(() {
                      if (on) {
                        _after.add(name);
                      } else {
                        _after.remove(name);
                      }
                    }),
                  ),
              ],
            ),
          ],
          const SizedBox(height: 8),
          Text(
            'It applies from ${widget.today}.',
            style: theme.textTheme.bodySmall,
          ),
          if (_blocked.isNotEmpty) ...[
            const SizedBox(height: 10),
            Text(
              _blocked,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.error,
              ),
            ),
          ],
          const SizedBox(height: 16),
          FilledButton(
            onPressed: _blocked.isEmpty
                ? () => Navigator.of(context).pop(
                    BookTaxDraft(
                      name: _name.text.trim(),
                      rate: _rate.text.trim(),
                      scope: _scope,
                      account: _account.text.trim(),
                      from: widget.today,
                      appliesAfter: _after.toList(),
                    ),
                  )
                : null,
            child: const Text('Save this tax'),
          ),
        ],
      ),
    );
  }
}
