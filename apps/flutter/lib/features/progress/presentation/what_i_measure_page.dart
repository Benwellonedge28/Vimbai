// What this Book measures: its figures as rows it owns, and the two acts that change them.
//
// ADR-017 made a measure a row rather than a code path, so a business can add what its own trade watches and drop what it
// does not, without a release. That only holds if the phone can reach the rows, which is what this screen is for.
//
// It says what each measure counts, not only what it came to: a person deciding what to take away needs to know what the
// row reads. The words are this screen's; the engine says what the row reads and nothing more.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/progress/data/measures_reading.dart';

class WhatIMeasurePage extends StatefulWidget {
  const WhatIMeasurePage({super.key, this.measures});

  /// A test passes its own service so the page can be exercised without a server.
  final BookMeasuresService? measures;

  @override
  State<WhatIMeasurePage> createState() => _WhatIMeasurePageState();
}

class _WhatIMeasurePageState extends State<WhatIMeasurePage> {
  late final BookMeasuresService _measures =
      widget.measures ?? BookMeasuresService.instance;

  List<MeasureRow> _rows = const [];
  bool _reading = true;
  String? _problem;

  @override
  void initState() {
    super.initState();
    _read();
  }

  String get _today {
    final now = DateTime.now();
    final month = now.month.toString().padLeft(2, '0');
    final day = now.day.toString().padLeft(2, '0');
    return '${now.year}-$month-$day';
  }

  Future<void> _read() async {
    setState(() {
      _reading = true;
      _problem = null;
    });
    try {
      final reading = await _measures.read(to: _today);
      if (!mounted) return;
      setState(() {
        _rows = reading.rows;
        _reading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _problem = error.toString().replaceFirst('Exception: ', '');
        _reading = false;
      });
    }
  }

  Future<void> _add() async {
    final measure = await showModalBottomSheet<Map<String, dynamic>>(
      context: context,
      isScrollControlled: true,
      builder: (context) => _AddMeasureSheet(measures: _rows),
    );
    if (measure == null || !mounted) return;
    try {
      await _measures.add(measure);
      if (!mounted) return;
      await _read();
    } catch (error) {
      _say(error.toString().replaceFirst('Exception: ', ''));
    }
  }

  Future<void> _drop(MeasureRow row) async {
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Stop measuring ${row.label}?'),
        content: const Text(
          'The figure leaves the screens. Nothing it counted is changed: the '
          'accounts and entries behind it stay exactly as they are.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Keep it'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Stop measuring it'),
          ),
        ],
      ),
    );
    if (sure != true || !mounted) return;
    try {
      await _measures.remove(row.name);
      if (!mounted) return;
      await _read();
    } catch (error) {
      _say(error.toString().replaceFirst('Exception: ', ''));
    }
  }

  void _say(String words) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(words)));
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('What I measure')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _reading ? null : _add,
        icon: const Icon(Icons.add),
        label: const Text('Add a measure'),
      ),
      body: RefreshIndicator(onRefresh: _read, child: _body(context)),
    );
  }

  Widget _body(BuildContext context) {
    final theme = Theme.of(context);
    if (_reading) return const Center(child: CircularProgressIndicator());
    if (_problem != null) {
      return ListView(
        padding: const EdgeInsets.all(16),
        children: [
          CouldNotRead(
            what: 'What this Book measures',
            error: _problem,
            onRetry: _read,
          ),
        ],
      );
    }
    if (_rows.isEmpty) {
      return ListView(
        padding: const EdgeInsets.all(24),
        children: [
          Text(
            'This Book measures nothing yet.',
            style: theme.textTheme.titleMedium,
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 8),
          Text(
            'Add a figure you watch, and it appears on How am I doing.',
            style: theme.textTheme.bodyMedium,
            textAlign: TextAlign.center,
          ),
        ],
      );
    }
    return ListView.separated(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 96),
      itemCount: _rows.length + 1,
      separatorBuilder: (context, index) => const SizedBox(height: 12),
      itemBuilder: (context, index) {
        if (index == 0) {
          return Text(
            'These are the figures this Book reads. Each one is a row, so '
            'adding or dropping one changes no code.',
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          );
        }
        return _MeasureCard(
          row: _rows[index - 1],
          onDrop: () => _drop(_rows[index - 1]),
        );
      },
    );
  }
}

/// One measure: what it is called, what it counts, and the way to stop measuring it.
class _MeasureCard extends StatelessWidget {
  const _MeasureCard({required this.row, required this.onDrop});

  final MeasureRow row;
  final VoidCallback onDrop;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: ListTile(
        title: Text(row.label.isEmpty ? row.name : row.label),
        subtitle: Text(howItIsWorkedOut(row)),
        isThreeLine: row.leads,
        trailing: IconButton(
          onPressed: onDrop,
          icon: const Icon(Icons.close),
          tooltip: 'Stop measuring ${row.label}',
        ),
      ),
    );
  }
}

/// What a measure counts, in a sentence.
String howItIsWorkedOut(MeasureRow row) {
  final reads = row.reads;
  final source = (reads['source'] as String?) ?? '';
  String joined(String key) {
    final held = reads[key];
    return held is List ? held.whereType<String>().join(', ') : '';
  }

  final said = switch (source) {
    'accounts' => () {
      final kinds = joined('types');
      final numbers = joined('numbers');
      final named = kinds.isNotEmpty ? kinds : numbers;
      final moneyOnly = reads['money_only'] == true;
      final held = reads['window'] == 'as_of';
      final what = named.isEmpty ? 'its accounts' : 'its $named accounts';
      final counted = moneyOnly ? 'What money $what' : 'What $what';
      return held
          ? '$counted held at the end'
          : '$counted moved over the period';
    }(),
    'documents' =>
      reads['documents'] == 'purchases'
          ? 'What is still owed on purchases'
          : 'What is still owed on sales',
    'tax' => 'What tax is set aside',
    'entries' =>
      reads['status'] == 'posted'
          ? 'How many entries are posted'
          : 'How many entries are not yet posted',
    'difference' => () {
      final from = (reads['of'] as String?) ?? '';
      final minus = joined('minus');
      if (from.isEmpty) return 'One measure less others';
      return minus.isEmpty ? '$from, on its own' : '$from less $minus';
    }(),
    'percent' => () {
      final of = (reads['of'] as String?) ?? '';
      final over = (reads['over'] as String?) ?? '';
      if (of.isEmpty || over.isEmpty)
        return 'One measure as a share of another';
      return '$of as a share of $over';
    }(),
    _ =>
      source.isEmpty ? 'A figure this Book reads' : 'Worked out from $source',
  };
  return row.leads ? '$said. Shown first.' : said;
}

/// The shapes a measure can take, in the words a person reads rather than the engine's.
const List<String> _shapes = [
  'accounts',
  'documents',
  'tax',
  'entries',
  'difference',
  'percent',
];

String _shapeWords(String shape) => switch (shape) {
  'accounts' => 'What accounts of one kind moved, or held',
  'documents' => 'What is still owed on sales or purchases',
  'tax' => 'What tax is set aside',
  'entries' => 'How many entries are in a state',
  'difference' => 'One measure less another',
  'percent' => 'One measure as a share of another',
  _ => shape,
};

/// Asking for a new measure, and sending the row the engine checks.
class _AddMeasureSheet extends StatefulWidget {
  const _AddMeasureSheet({required this.measures});

  /// The measures the Book already holds, because a measure worked out from others has to name ones that exist.
  final List<MeasureRow> measures;

  @override
  State<_AddMeasureSheet> createState() => _AddMeasureSheetState();
}

class _AddMeasureSheetState extends State<_AddMeasureSheet> {
  final TextEditingController _label = TextEditingController();
  String _shape = 'accounts';
  String _accountKind = 'expense';
  bool _heldAtTheEnd = false;
  bool _moneyOnly = false;
  String _documents = 'sales';
  String _entryState = 'draft';
  String _from = '';
  String _minus = '';
  String _over = '';
  bool _leads = false;
  String? _problem;

  @override
  void initState() {
    super.initState();
    final names = [for (final row in widget.measures) row.name];
    _from = names.isNotEmpty ? names.first : '';
    _minus = names.length > 1
        ? names[1]
        : (names.isNotEmpty ? names.first : '');
    _over = names.isNotEmpty ? names.first : '';
  }

  @override
  void dispose() {
    _label.dispose();
    super.dispose();
  }

  /// The row's name, from the words the person typed: `Marketing spend` becomes `marketing_spend`.
  String get _name => _label.text
      .trim()
      .toLowerCase()
      .replaceAll(RegExp(r'[^a-z0-9]+'), '_')
      .replaceAll(RegExp(r'^_+|_+$'), '');

  Map<String, dynamic> _row() {
    final row = <String, dynamic>{
      'name': _name,
      'label': _label.text.trim(),
      'unit': 'money',
      'source': _shape,
      'leads': _leads,
    };
    switch (_shape) {
      case 'accounts':
        row['types'] = [_accountKind];
        row['window'] = _heldAtTheEnd ? 'as_of' : 'period';
        if (_moneyOnly) row['money_only'] = true;
      case 'documents':
        row['documents'] = _documents;
      case 'entries':
        row['status'] = _entryState;
        row['unit'] = 'count';
      case 'difference':
        row['of'] = _from;
        row['minus'] = [_minus];
      case 'percent':
        row['of'] = _from;
        row['over'] = _over;
        row['unit'] = 'percent';
      default:
        break;
    }
    return row;
  }

  void _send() {
    if (_label.text.trim().isEmpty) {
      setState(() => _problem = 'Say what the measure is called.');
      return;
    }
    if ((_shape == 'difference' || _shape == 'percent') &&
        widget.measures.isEmpty) {
      setState(
        () => _problem =
            'This Book measures nothing yet, so there is nothing to work '
            'from. Add a plain measure first.',
      );
      return;
    }
    Navigator.pop(context, _row());
  }

  @override
  Widget build(BuildContext context) {
    final names = [for (final row in widget.measures) row.name];
    return Padding(
      padding: EdgeInsets.only(
        left: 16,
        right: 16,
        top: 16,
        bottom: MediaQuery.of(context).viewInsets.bottom + 16,
      ),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'Add a measure',
              style: Theme.of(context).textTheme.titleLarge,
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _label,
              decoration: const InputDecoration(
                labelText: 'What it is called',
                hintText: 'Marketing spend',
              ),
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<String>(
              isExpanded: true,
              initialValue: _shape,
              decoration: const InputDecoration(labelText: 'What it counts'),
              items: [
                for (final shape in _shapes)
                  DropdownMenuItem(
                    value: shape,
                    child: Text(_shapeWords(shape)),
                  ),
              ],
              onChanged: (chosen) =>
                  setState(() => _shape = chosen ?? 'accounts'),
            ),
            const SizedBox(height: 12),
            if (_shape == 'accounts') ...[
              DropdownButtonFormField<String>(
                isExpanded: true,
                initialValue: _accountKind,
                decoration: const InputDecoration(labelText: 'Which accounts'),
                items: const [
                  DropdownMenuItem(value: 'asset', child: Text('assets')),
                  DropdownMenuItem(
                    value: 'liability',
                    child: Text('liabilities'),
                  ),
                  DropdownMenuItem(value: 'equity', child: Text('equity')),
                  DropdownMenuItem(value: 'revenue', child: Text('revenue')),
                  DropdownMenuItem(value: 'expense', child: Text('expenses')),
                ],
                onChanged: (chosen) =>
                    setState(() => _accountKind = chosen ?? 'expense'),
              ),
              SwitchListTile(
                value: _heldAtTheEnd,
                onChanged: (on) => setState(() => _heldAtTheEnd = on),
                title: const Text('As it stands at the end, not what moved'),
              ),
              SwitchListTile(
                value: _moneyOnly,
                onChanged: (on) => setState(() => _moneyOnly = on),
                title: const Text('Only the money accounts'),
              ),
            ],
            if (_shape == 'documents')
              DropdownButtonFormField<String>(
                isExpanded: true,
                initialValue: _documents,
                decoration: const InputDecoration(labelText: 'Which documents'),
                items: const [
                  DropdownMenuItem(value: 'sales', child: Text('sales')),
                  DropdownMenuItem(
                    value: 'purchases',
                    child: Text('purchases'),
                  ),
                ],
                onChanged: (chosen) =>
                    setState(() => _documents = chosen ?? 'sales'),
              ),
            if (_shape == 'entries')
              DropdownButtonFormField<String>(
                isExpanded: true,
                initialValue: _entryState,
                decoration: const InputDecoration(labelText: 'Which entries'),
                items: const [
                  DropdownMenuItem(
                    value: 'draft',
                    child: Text('not yet posted'),
                  ),
                  DropdownMenuItem(value: 'posted', child: Text('posted')),
                ],
                onChanged: (chosen) =>
                    setState(() => _entryState = chosen ?? 'draft'),
              ),
            if (_shape == 'difference' || _shape == 'percent') ...[
              DropdownButtonFormField<String>(
                isExpanded: true,
                initialValue: _from.isEmpty ? null : _from,
                decoration: InputDecoration(
                  labelText: _shape == 'percent'
                      ? 'The measure to take a share of'
                      : 'The measure to start from',
                ),
                items: [
                  for (final name in names)
                    DropdownMenuItem(value: name, child: Text(name)),
                ],
                onChanged: (chosen) => setState(() => _from = chosen ?? ''),
              ),
              const SizedBox(height: 12),
              DropdownButtonFormField<String>(
                isExpanded: true,
                initialValue: _shape == 'percent'
                    ? (_over.isEmpty ? null : _over)
                    : (_minus.isEmpty ? null : _minus),
                decoration: InputDecoration(
                  labelText: _shape == 'percent'
                      ? 'The measure it is a share of'
                      : 'The measure to take away',
                ),
                items: [
                  for (final name in names)
                    DropdownMenuItem(value: name, child: Text(name)),
                ],
                onChanged: (chosen) => setState(() {
                  if (_shape == 'percent') {
                    _over = chosen ?? '';
                  } else {
                    _minus = chosen ?? '';
                  }
                }),
              ),
            ],
            SwitchListTile(
              value: _leads,
              onChanged: (on) => setState(() => _leads = on),
              title: const Text('Show it first'),
            ),
            if (_problem != null) ...[
              const SizedBox(height: 8),
              Text(
                _problem!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
            const SizedBox(height: 12),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: () => Navigator.pop(context),
                  child: const Text('Leave it'),
                ),
                const SizedBox(width: 8),
                FilledButton(onPressed: _send, child: const Text('Add it')),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
