// Putting the wear on what a business owns into its books.
//
// The register screen ("Things you own") already charges a month: it works out what that month cost, takes it off the
// asset's value and records the row. None of that reached the ledger, so a Book's profit included no wear at all and its
// assets sat at what they cost on the day they were bought.
//
// This screen is the other half, and it is deliberately the smallest thing that does the job: pick a month, see what the
// register recorded for it, and post it. It works nothing out. A charge computed here as well as there would be two
// arithmetics for one figure, and on the day they disagree nobody could say which one the books meant.
//
// What it says before anything is written, because it is the part people ask about: no money moves. The thing stays in the
// books at what it cost, and what has been used up is kept beside it.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/depreciation/data/depreciation_posting.dart';

class PostDepreciationPage extends StatefulWidget {
  const PostDepreciationPage({super.key, this.posting});

  /// A test passes its own service so the page can be exercised without a server.
  final DepreciationPostingService? posting;

  @override
  State<PostDepreciationPage> createState() => _PostDepreciationPageState();
}

class _PostDepreciationPageState extends State<PostDepreciationPage> {
  late final DepreciationPostingService _posting =
      widget.posting ?? DepreciationPostingService.instance;

  bool _reading = true;
  bool _busy = false;
  String? _trouble;

  /// What the register has recorded and the books have not.
  DepreciationWaiting _waiting = const DepreciationWaiting(
    charges: [],
    total: '',
  );

  /// The month being posted. Empty until the waiting list says which months there are.
  String _period = '';

  /// The last month posted, so the screen can say what went in.
  DepreciationPosted? _done;

  @override
  void initState() {
    super.initState();
    _read();
  }

  Future<void> _read() async {
    setState(() {
      _reading = true;
      _trouble = null;
    });
    try {
      final waiting = await _posting.waiting();
      if (!mounted) return;
      setState(() {
        _waiting = waiting;
        // the oldest month waiting is the one to post first: a Book catches up in order rather than jumping about
        if (waiting.months.isNotEmpty) {
          _period = waiting.months.first;
        } else if (_period.isEmpty) {
          _period = _lastMonth();
        }
        _reading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _trouble = CouldNotRead.reasonOf(error);
        _reading = false;
      });
    }
  }

  /// The month that has just ended, as `2026-11`, because a month's wear is known once the month is over.
  String _lastMonth() {
    final today = DateTime.now();
    final last = DateTime(today.year, today.month - 1, 1);
    final month = last.month.toString().padLeft(2, '0');
    return '${last.year}-$month';
  }

  /// What the register recorded for the chosen month, oldest name first so the list does not jump about between reads.
  List<DepreciationCharge> get _forPeriod {
    final charges = _waiting.charges
        .where((charge) => charge.period == _period)
        .toList();
    charges.sort(
      (a, b) => a.called.toLowerCase().compareTo(b.called.toLowerCase()),
    );
    return charges;
  }

  /// The months a person can choose from: the ones waiting, and the month just ended when nothing is waiting.
  List<String> get _months {
    final months = _waiting.months.toList();
    if (months.isEmpty) months.add(_lastMonth());
    if (!months.contains(_period) && _period.isNotEmpty) months.add(_period);
    months.sort();
    return months;
  }

  Future<void> _post() async {
    setState(() {
      _busy = true;
      _trouble = null;
    });
    try {
      final posted = await _posting.postMonth(_period);
      if (!mounted) return;
      setState(() {
        _done = posted;
        _busy = false;
      });
      await _read();
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _trouble = CouldNotRead.reasonOf(error);
        _busy = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final charges = _forPeriod;
    final done = _done;
    return Scaffold(
      appBar: AppBar(title: const Text('Wear on what you own')),
      body: _reading
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
              children: [
                Text(
                  'Nothing leaves the bank for this. The thing stays in your books at what it cost, and what it has '
                  'been used up by is kept beside it, so your profit carries the wear instead of the day you bought it.',
                  style: theme.textTheme.bodyMedium,
                ),
                const SizedBox(height: 16),
                if (_waiting.isEmpty && done == null) ...[
                  Text(
                    'Nothing is waiting. A month reaches this screen once it has been charged in "Things you own": '
                    'the register works out what that month cost, and this puts it into your books.',
                    style: theme.textTheme.bodyMedium,
                  ),
                  const SizedBox(height: 16),
                ],
                DropdownButtonFormField<String>(
                  initialValue: _period.isEmpty ? null : _period,
                  decoration: const InputDecoration(
                    labelText: 'Which month',
                    border: OutlineInputBorder(),
                  ),
                  items: [
                    for (final month in _months)
                      DropdownMenuItem(value: month, child: Text(month)),
                  ],
                  onChanged: _busy
                      ? null
                      : (value) => setState(() {
                          _period = value ?? _period;
                          _done = null;
                        }),
                ),
                const SizedBox(height: 16),
                if (charges.isNotEmpty) ...[
                  Text(
                    'The register recorded ${charges.length} '
                    '${charges.length == 1 ? 'asset' : 'assets'} for $_period:',
                    style: theme.textTheme.titleSmall,
                  ),
                  const SizedBox(height: 8),
                  for (final charge in charges)
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      dense: true,
                      title: Text(charge.called),
                      trailing: Text(
                        charge.amount,
                        style: theme.textTheme.bodyMedium,
                      ),
                    ),
                  const SizedBox(height: 8),
                ],
                if (charges.isEmpty && _waiting.isNotEmpty)
                  Text(
                    'The register recorded nothing for $_period. Choose one of the months it is waiting on.',
                    style: theme.textTheme.bodyMedium,
                  ),
                if (_waiting.charges.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  Text(
                    'Waiting to reach your books: ${_waiting.total} across '
                    '${_waiting.months.length} '
                    '${_waiting.months.length == 1 ? 'month' : 'months'}.',
                    style: theme.textTheme.bodySmall,
                  ),
                ],
                const SizedBox(height: 20),
                FilledButton(
                  onPressed: _busy || charges.isEmpty ? null : _post,
                  child: Text(
                    _busy
                        ? 'Posting...'
                        : charges.isEmpty
                        ? 'Nothing to post for $_period'
                        : 'Post $_period to the books',
                  ),
                ),
                if (done != null) ...[
                  const SizedBox(height: 20),
                  Card(
                    child: Padding(
                      padding: const EdgeInsets.all(12),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            done.charged.isEmpty
                                ? '${done.period} was already in your books.'
                                : '${done.period} is in your books: ${done.total} of wear.',
                            style: theme.textTheme.titleSmall,
                          ),
                          if (done.charged.isNotEmpty) ...[
                            const SizedBox(height: 6),
                            Text(
                              'One entry per asset, so you can see which charge belongs to which thing.',
                              style: theme.textTheme.bodySmall,
                            ),
                            for (final charge in done.charged)
                              ListTile(
                                contentPadding: EdgeInsets.zero,
                                dense: true,
                                title: Text(charge.called),
                                trailing: Text(charge.amount),
                              ),
                          ],
                          if (done.alreadyPosted.isNotEmpty) ...[
                            const SizedBox(height: 6),
                            Text(
                              '${done.alreadyPosted.length} '
                              '${done.alreadyPosted.length == 1 ? 'asset was' : 'assets were'} already in, so posting '
                              'again changed nothing.',
                              style: theme.textTheme.bodySmall,
                            ),
                          ],
                        ],
                      ),
                    ),
                  ),
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
              ],
            ),
    );
  }
}
