// The first day of a business's own books: what it already held, and where that came from.
//
// A business rarely starts from nothing. It arrives holding stock, money in the bank, money owed to it, and owing a
// supplier and a lender, with capital that has been building for years. Those figures are the first thing its books have
// to know, because every month afterwards is measured from them: a profit means more than what was there on the first day.
//
// The screen asks in the words a person uses rather than the ledger's: what the business holds, and where it came from.
// Everything it holds came from somewhere - someone lent it, someone is owed it, or the owners put it in - so the two sides
// have to come to the same figure, and the screen says how far apart they are while they are being typed rather than
// letting the engine refuse at the end.
//
// It adds nothing up to make a set balance and it picks no account: the person names the accounts the Book already keeps,
// and the engine refuses a set that does not balance, an account the chart does not have, and a second first day.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/account_picker_sheet.dart';
import 'package:vimbai_mobile_client/features/opening_balances/data/opening_balances_posting.dart';

/// One line the person is stating: an account, which side it sits on, and what it comes to.
class _Row {
  _Row() : amount = TextEditingController();

  String account = '';

  /// True when this is something the business owes or the owners have in it, rather than something it holds.
  bool cameFrom = false;
  final TextEditingController amount;

  void dispose() => amount.dispose();
}

class YourFirstDayPage extends StatefulWidget {
  const YourFirstDayPage({super.key, this.posting, this.ledger});

  /// A test passes its own services so the page can be exercised without a server.
  final OpeningBalancesPostingService? posting;
  final AccountingApiService? ledger;

  @override
  State<YourFirstDayPage> createState() => _YourFirstDayPageState();
}

class _YourFirstDayPageState extends State<YourFirstDayPage> {
  late final OpeningBalancesPostingService _posting =
      widget.posting ?? OpeningBalancesPostingService.instance;
  late final AccountingApiService _ledger =
      widget.ledger ?? AccountingApiService();

  final TextEditingController _day = TextEditingController(text: _today());
  final TextEditingController _notes = TextEditingController();
  // one line on each side to begin with, because a set that balances has both
  final List<_Row> _rows = [_Row(), _Row()..cameFrom = true];

  List<Account> _accounts = const [];
  OpeningSet _stated = OpeningSet.empty;
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
    for (final row in _rows) {
      row.dispose();
    }
    _day.dispose();
    _notes.dispose();
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
      final chart = await _ledger.getAllAccounts();
      final stated = await _posting.read();
      if (!mounted) return;
      setState(() {
        _accounts = [...chart]
          ..sort((a, b) => a.accountNumber.compareTo(b.accountNumber));
        _stated = stated;
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

  /// What a row states, or nothing when it is blank or not a figure yet.
  ExactAmount? _figureOf(_Row row) {
    final typed = row.amount.text.trim();
    if (typed.isEmpty) return null;
    try {
      return ExactAmount.parse(typed);
    } catch (_) {
      return null;
    }
  }

  /// What the business holds, as the rows state it.
  ExactAmount get _holds {
    var total = ExactAmount.zero;
    for (final row in _rows) {
      if (row.cameFrom) continue;
      final figure = _figureOf(row);
      if (figure != null) total = total + figure;
    }
    return total;
  }

  /// Where it all came from, as the rows state it.
  ExactAmount get _cameFrom {
    var total = ExactAmount.zero;
    for (final row in _rows) {
      if (!row.cameFrom) continue;
      final figure = _figureOf(row);
      if (figure != null) total = total + figure;
    }
    return total;
  }

  /// How far apart the two sides are, said as a figure rather than as a word.
  ExactAmount get _apart {
    final holds = _holds;
    final cameFrom = _cameFrom;
    if (holds.compareTo(cameFrom) >= 0) return holds - cameFrom;
    return cameFrom - holds;
  }

  bool get _agrees => _apart.isZero && !_holds.isZero;

  /// The account as the chart names it, so a row reads the way the chart does.
  String _named(String number) {
    for (final account in _accounts) {
      if (account.accountNumber == number) {
        return '${account.accountNumber} · ${account.accountName}';
      }
    }
    return number;
  }

  /// Everything that would stop the engine posting, said before it is asked.
  String? get _refusal {
    if (!RegExp(r'^\d{4}-\d{2}-\d{2}$').hasMatch(_day.text.trim())) {
      return 'Say the first day these books cover, as 2026-01-01.';
    }
    for (final row in _rows) {
      final typed = row.amount.text.trim();
      final named = row.account.isNotEmpty;
      if (typed.isEmpty && !named) continue;
      if (!named) {
        return 'One line has a figure but no account. Say which account it belongs to.';
      }
      if (typed.isEmpty) {
        return 'One line names an account but states nothing. Say the figure, or remove the line.';
      }
      final figure = _figureOf(row);
      if (figure == null) {
        return 'One line is not a figure a Book can carry. Say it as 1200.00.';
      }
      if (figure.isZero) {
        return 'One line states nothing. Leave it out rather than stating a line of nothing.';
      }
    }
    if (_holds.isZero && _cameFrom.isZero) {
      return 'State at least one thing the business already had or already owed.';
    }
    if (!_agrees) {
      return 'The two sides are ${_apart.toDecimalString()} apart. Everything a business holds came from '
          'somewhere: find the figure that is missing rather than making the sides agree.';
    }
    return null;
  }

  Future<void> _post() async {
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
      final lines = <OpeningLine>[
        for (final row in _rows)
          if (row.account.isNotEmpty && _figureOf(row) != null)
            OpeningLine(
              accountNumber: row.account,
              debit: row.cameFrom ? '' : row.amount.text.trim(),
              credit: row.cameFrom ? row.amount.text.trim() : '',
            ),
      ];
      final stated = await _posting.post(
        entryDate: _day.text.trim(),
        lines: lines,
        notes: _notes.text,
      );
      if (!mounted) return;
      setState(() {
        _stated = stated;
        _saving = false;
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
      appBar: AppBar(title: const Text('Your first day')),
      body: _reading
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
              children: [
                Text(
                  'What the business already had and already owed on the day these books start. Every month after this '
                  'one is measured from it, so a profit means more than what was there on the first day.',
                  style: theme.textTheme.bodyMedium,
                ),
                const SizedBox(height: 16),
                if (_stated.isStated)
                  ..._alreadyStated(theme)
                else
                  ..._form(theme),
              ],
            ),
    );
  }

  /// A Book has one first day, so once it is stated the screen reads it back rather than offering it again.
  List<Widget> _alreadyStated(ThemeData theme) => [
    Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'These books start on ${_stated.entryDate}',
              style: theme.textTheme.titleSmall,
            ),
            const SizedBox(height: 6),
            Text(
              '${_stated.debits} held, ${_stated.credits} from where it came. '
              'A correction is a reversing entry, never an edit.',
              style: theme.textTheme.bodySmall,
            ),
          ],
        ),
      ),
    ),
    const SizedBox(height: 20),
    Text('What was stated', style: theme.textTheme.titleSmall),
    const SizedBox(height: 6),
    for (final line in _stated.accounts)
      ListTile(
        contentPadding: EdgeInsets.zero,
        dense: true,
        title: Text(_named(line.accountNumber)),
        subtitle: Text(line.side),
        trailing: Text(line.figure),
      ),
    if (_stated.notes.trim().isNotEmpty) ...[
      const SizedBox(height: 8),
      Text(_stated.notes, style: theme.textTheme.bodySmall),
    ],
    if (_trouble != null) ...[
      const SizedBox(height: 16),
      Text(
        _trouble!,
        style: theme.textTheme.bodyMedium?.copyWith(
          color: theme.colorScheme.error,
        ),
      ),
    ],
  ];

  List<Widget> _form(ThemeData theme) => [
    TextField(
      controller: _day,
      decoration: const InputDecoration(
        labelText: 'The first day',
        helperText: 'As 2026-01-01: the day these books begin.',
        border: OutlineInputBorder(),
      ),
      onChanged: (value) => setState(() {}),
    ),
    const SizedBox(height: 20),
    Text('What the business holds', style: theme.textTheme.titleSmall),
    const SizedBox(height: 4),
    Text(
      'Money in the bank, cash, stock, what customers owe it, equipment.',
      style: theme.textTheme.bodySmall,
    ),
    const SizedBox(height: 8),
    ..._rowsFor(theme, holds: true),
    const SizedBox(height: 20),
    Text('Where it came from', style: theme.textTheme.titleSmall),
    const SizedBox(height: 4),
    Text(
      'What it owes a supplier or a lender, and what the owners have in it. The owners\' share is what is left over.',
      style: theme.textTheme.bodySmall,
    ),
    const SizedBox(height: 8),
    ..._rowsFor(theme, holds: false),
    const SizedBox(height: 12),
    Row(
      children: [
        Expanded(
          child: Text(
            '${_holds.toDecimalString()} held · ${_cameFrom.toDecimalString()} from where it came',
            style: theme.textTheme.bodySmall,
          ),
        ),
      ],
    ),
    const SizedBox(height: 6),
    Text(
      _agrees
          ? 'The two sides agree.'
          : '${_apart.toDecimalString()} apart. Everything a business holds came from somewhere.',
      style: theme.textTheme.bodyMedium?.copyWith(
        color: _agrees ? theme.colorScheme.primary : theme.colorScheme.error,
      ),
    ),
    const SizedBox(height: 12),
    TextField(
      controller: _notes,
      decoration: const InputDecoration(
        labelText: 'Where these figures come from',
        helperText: 'Optional: from the old books, from the accountant.',
        border: OutlineInputBorder(),
      ),
    ),
    const SizedBox(height: 20),
    FilledButton(
      onPressed: _saving ? null : _post,
      child: Text(_saving ? 'Stating...' : 'Open the books on this day'),
    ),
    const SizedBox(height: 8),
    Text(
      'Nothing moves: this states what the business already had, it does not put money anywhere.',
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
  ];

  /// The rows on one side of the books, with a way to state another.
  List<Widget> _rowsFor(ThemeData theme, {required bool holds}) {
    final side = [
      for (final row in _rows)
        if (row.cameFrom != holds) row,
    ];
    return [
      for (final row in side) ...[
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(
              child: InkWell(
                onTap: () async {
                  final picked = await showAccountPicker(
                    context,
                    accounts: _accounts,
                  );
                  if (picked == null || !mounted) return;
                  setState(() => row.account = picked.accountNumber);
                },
                child: InputDecorator(
                  decoration: const InputDecoration(
                    labelText: 'Account',
                    border: OutlineInputBorder(),
                  ),
                  child: Text(
                    row.account.isEmpty ? 'Choose' : _named(row.account),
                  ),
                ),
              ),
            ),
            const SizedBox(width: 8),
            SizedBox(
              width: 130,
              child: TextField(
                controller: row.amount,
                keyboardType: const TextInputType.numberWithOptions(
                  decimal: true,
                ),
                decoration: const InputDecoration(
                  labelText: 'Figure',
                  border: OutlineInputBorder(),
                ),
                onChanged: (value) => setState(() {}),
              ),
            ),
            IconButton(
              tooltip: 'Remove this line',
              onPressed: () => setState(() {
                _rows.remove(row);
                row.dispose();
                if (_rows.isEmpty) _rows.add(_Row());
              }),
              icon: const Icon(Icons.close),
            ),
          ],
        ),
        const SizedBox(height: 8),
      ],
      TextButton.icon(
        onPressed: () => setState(() {
          final row = _Row()..cameFrom = !holds;
          _rows.add(row);
        }),
        icon: const Icon(Icons.add),
        label: Text(holds ? 'Another thing it holds' : 'Another thing it owes'),
      ),
    ];
  }
}
