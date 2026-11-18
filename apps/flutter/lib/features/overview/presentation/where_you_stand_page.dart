// Where you stand: the four numbers a business owner actually opens the app for.
//
// Every piece of this Book already exists on its own screen - the cash book, the invoices, what you owe, the income
// statement - but answering "how is the business doing" meant opening four of them and holding the answers in your head.
// This page is that one look: money held, money owed to you, money you owe, and what you have made this year.
//
// Each figure comes from the door that owns it, and each card opens that door. Nothing is added up on the phone that a door
// already adds up. A card whose read failed says so in place, because a page that blanks itself when one of four reads is
// slow tells a person nothing about the other three.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/income_statement_page.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/cash_book_page.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/invoices_page.dart';
import 'package:vimbai_mobile_client/features/overview/data/stand_source.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/payables_page.dart';

class WhereYouStandPage extends StatefulWidget {
  const WhereYouStandPage({super.key, this.source, this.year});

  /// A test answers the four figures itself, so the page can be exercised without a server.
  final WhereYouStandSource? source;

  /// A test fixes the year the profit is counted from.
  final int? year;

  @override
  State<WhereYouStandPage> createState() => _WhereYouStandPageState();
}

/// One figure, or the reason it could not be read.
class _Reading {
  const _Reading.known(this.amount) : reason = null;
  const _Reading.unknown(this.reason) : amount = null;

  final ExactAmount? amount;
  final String? reason;

  bool get isKnown => amount != null;
}

class _WhereYouStandPageState extends State<WhereYouStandPage> {
  late final WhereYouStandSource _source = widget.source ?? BookStandSource();

  late final int _year = widget.year ?? DateTime.now().year;

  bool _loading = true;
  _Reading? _held;
  _Reading? _owed;
  _Reading? _owing;
  _Reading? _profit;

  @override
  void initState() {
    super.initState();
    _load();
  }

  /// One reading, with a failure kept as an answer rather than thrown away.
  Future<_Reading> _ask(Future<ExactAmount> reading) async {
    try {
      return _Reading.known(await reading);
    } catch (error) {
      return _Reading.unknown(CouldNotRead.reasonOf(error));
    }
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
    });
    // all four at once: none of them needs another's answer, and waiting for them one after another would make the page
    // slower than its slowest door four times over
    final answers = await Future.wait([
      _ask(_source.moneyHeld()),
      _ask(_source.owedToYou()),
      _ask(_source.youOwe()),
      _ask(_source.profitThisYear()),
    ]);
    if (!mounted) return;
    setState(() {
      _held = answers[0];
      _owed = answers[1];
      _owing = answers[2];
      _profit = answers[3];
      _loading = false;
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Where you stand'),
        actions: [
          IconButton(
            tooltip: 'Ask again',
            onPressed: _loading ? null : _load,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : RefreshIndicator(
              onRefresh: _load,
              child: ListView(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
                children: [
                  _opening(),
                  const SizedBox(height: 12),
                  ..._card(
                    label: 'Money held',
                    reading: _held,
                    note: 'In cash and at the bank, as the cash book has it.',
                    what: 'The cash position',
                    icon: Icons.account_balance_wallet_outlined,
                    open: const CashBookPage(),
                  ),
                  ..._card(
                    label: 'Owed to you',
                    reading: _owed,
                    note:
                        'What customers still have to pay on invoices raised.',
                    what: 'The invoice register',
                    icon: Icons.receipt_long,
                    open: const InvoicesPage(),
                  ),
                  ..._card(
                    label: 'You owe',
                    reading: _owing,
                    note: 'What is still to go out to suppliers.',
                    what: 'The purchases ledger',
                    icon: Icons.receipt,
                    open: const PayablesPage(),
                  ),
                  ..._card(
                    label: 'Profit this year',
                    reading: _profit,
                    note:
                        'Income less expenses since 1 January $_year, '
                        'before anything taken out by the owner.',
                    what: 'The income statement',
                    icon: Icons.trending_up,
                    open: const IncomeStatementPage(),
                  ),
                ],
              ),
            ),
    );
  }

  /// The one sentence that says what the page is, so the four figures are not four unlabelled numbers.
  Widget _opening() {
    final held = _held;
    final owed = _owed;
    final owing = _owing;
    if (held == null || owed == null || owing == null) {
      return const SizedBox.shrink();
    }
    if (!held.isKnown || !owed.isKnown || !owing.isKnown) {
      return const SizedBox.shrink();
    }
    // money in hand plus money coming in, less money going out: the three that can be counted today
    final inHand = held.amount! + owed.amount! - owing.amount!;
    return Text(
      'This Book holds ${held.amount!.toDecimalString()}, is owed '
      '${owed.amount!.toDecimalString()}, and owes '
      '${owing.amount!.toDecimalString()}. Counted together that is '
      '${inHand.toDecimalString()}.',
      style: Theme.of(context).textTheme.titleSmall,
    );
  }

  List<Widget> _card({
    required String label,
    required _Reading? reading,
    required String note,
    required String what,
    required IconData icon,
    required Widget open,
  }) {
    return [
      Card(
        clipBehavior: Clip.antiAlias,
        child: InkWell(
          onTap: () => Navigator.of(
            context,
          ).push(MaterialPageRoute<void>(builder: (_) => open)),
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(icon, size: 28),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        label,
                        style: Theme.of(context).textTheme.labelLarge,
                      ),
                      const SizedBox(height: 4),
                      if (reading == null)
                        const Text('Not asked yet.')
                      else if (reading.isKnown)
                        Text(
                          reading.amount!.toDecimalString(),
                          style: Theme.of(context).textTheme.headlineSmall
                              ?.copyWith(fontWeight: FontWeight.bold),
                        )
                      else
                        Text(
                          '$what could not be read just now. ${reading.reason}',
                          style: TextStyle(
                            color: Theme.of(context).colorScheme.error,
                          ),
                        ),
                      const SizedBox(height: 6),
                      Text(note),
                    ],
                  ),
                ),
                const Icon(Icons.chevron_right),
              ],
            ),
          ),
        ),
      ),
      const SizedBox(height: 8),
    ];
  }
}
