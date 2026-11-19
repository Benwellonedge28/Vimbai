// One supplier's account: what was billed, what was paid, and what was left after each.
//
// The mirror of a customer statement, for the other direction. A balance on the "What you owe" page says how much a
// supplier is owed; it does not say how it got there, and that is the question somebody asks before paying: is this
// month's bill on it, did my payment land, why is it higher than I expected.
//
// The engine answers the whole account in one request, including the closing balance, so this page prints what it was told
// rather than adding the lines up itself. A page that summed its own lines could disagree with the balance the Book
// reports, and two figures for one debt is worse than a slow screen.
import 'package:flutter/material.dart';
import 'package:uuid/uuid.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/models/journal_entry_drafting.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/payables/data/bill_posting.dart';
import 'package:vimbai_mobile_client/features/payables/data/supplier_account.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/pay_supplier_out_sheet.dart';
import 'package:vimbai_mobile_client/features/tax/data/taxed_amounts.dart';

class SupplierAccountPage extends StatefulWidget {
  const SupplierAccountPage({
    super.key,
    required this.supplierId,
    this.supplierName,
    this.service,
    this.ledger,
    this.cash,
  });

  final String supplierId;
  final String? supplierName;

  /// A test passes its own service so the page can be exercised without a server.
  final SupplierAccountService? service;

  /// The ledger, for posting a bill into the books. A test passes its own.
  final AccountingApiService? ledger;

  /// The cash book, for recording the money a payment took out of the bank.
  final CashBookService? cash;

  @override
  State<SupplierAccountPage> createState() => _SupplierAccountPageState();
}

class _SupplierAccountPageState extends State<SupplierAccountPage> {
  late final SupplierAccountService _accounts =
      widget.service ?? SupplierAccountService.instance;
  late final AccountingApiService _ledger =
      widget.ledger ?? AccountingApiService();
  late final CashBookService _cash = widget.cash ?? CashBookService();

  bool _posting = false;

  bool _loading = true;
  String? _error;
  SupplierAccount? _account;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final account = await _accounts.account(widget.supplierId);
      if (!mounted) return;
      setState(() {
        _account = account;
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

  /// Put a bill, or a payment, into the accounts.
  ///
  /// The purchases ledger answers who is owed what; the accounts answer what the Book earned and what it holds. A bill
  /// that lives only in the first makes the profit figure and the balance sheet both wrong by the same amount, so this
  /// walks the one route into the ledger: the chart proposes both sides, the person sees the entry before it is written,
  /// and the bill's own reference travels with it so tapping twice cannot post it twice - the engine refuses the second
  /// one by name.
  Future<void> _postToTheBooks(
    SupplierAccountLine line,
    String supplierName,
  ) async {
    setState(() => _posting = true);
    try {
      final chart = await _ledger.getAllAccounts();
      final isBill = line.kind == 'purchase_invoice';
      if (!isBill) {
        // Money leaving is a cash movement, and a cash movement has one route into the books: the payment itself, which
        // carries its own reference. Posting a journal entry here as well would record the same money twice.
        setState(() => _posting = false);
        await _recordTheMoneyLeaving(line, supplierName);
        return;
      }
      final reference = line.reference.trim().isEmpty
          ? '${isBill ? 'BILL' : 'PAY'}-${line.date}-${widget.supplierId}'
          : line.reference.trim();
      final taxField = TextEditingController();
      Account? chosen;
      if (isBill) {
        final costs = chargeableAccounts(chart);
        if (costs.isNotEmpty) chosen = costs.first;
      }
      var posting = billPosting(
        chart: chart,
        amount: line.amount,
        supplierName: supplierName,
        reference: reference,
        charged: chosen,
      );
      if (!mounted) return;
      if (!posting.canPost) {
        setState(() => _posting = false);
        _say(posting.refusal);
        return;
      }
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (ctx) => StatefulBuilder(
          builder: (ctx, setSheet) => AlertDialog(
            title: const Text('Post this bill'),
            content: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final side in posting.sides) Text(side.line),
                if (isBill && taxAccountIn(chart) != null) ...[
                  const SizedBox(height: 12),
                  TextField(
                    controller: taxField,
                    keyboardType: const TextInputType.numberWithOptions(
                      decimal: true,
                    ),
                    decoration: InputDecoration(
                      labelText: 'Tax on this bill',
                      helperText:
                          'Leave it empty if the bill carries none. It is posted to '
                          '${taxAccountIn(chart)!.accountNumber} '
                          '${taxAccountIn(chart)!.accountName} to reclaim.',
                    ),
                    onChanged: (typed) {
                      setSheet(() {
                        posting = billPosting(
                          chart: chart,
                          amount: line.amount,
                          supplierName: supplierName,
                          reference: reference,
                          charged: chosen,
                          tax: _taxFrom(typed),
                        );
                      });
                    },
                  ),
                ],
                if (chargeableAccounts(chart).length > 1) ...[
                  const SizedBox(height: 12),
                  DropdownButtonFormField<Account>(
                    initialValue: chosen,
                    decoration: const InputDecoration(
                      labelText: 'What was it for',
                    ),
                    items: [
                      for (final account in chargeableAccounts(chart))
                        DropdownMenuItem(
                          value: account,
                          child: Text(
                            '${account.accountNumber} ${account.accountName}',
                          ),
                        ),
                    ],
                    onChanged: (picked) {
                      if (picked == null) return;
                      setSheet(() {
                        chosen = picked;
                        posting = billPosting(
                          chart: chart,
                          amount: line.amount,
                          supplierName: supplierName,
                          reference: reference,
                          charged: picked,
                          tax: _taxFrom(taxField.text),
                        );
                      });
                    },
                  ),
                ],
                const SizedBox(height: 12),
                Text(
                  'Reference $reference. A posted entry is corrected by reversing '
                  'it, never edited.',
                  style: Theme.of(ctx).textTheme.bodySmall,
                ),
              ],
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.of(ctx).pop(false),
                child: const Text('Not now'),
              ),
              ElevatedButton(
                onPressed: () => Navigator.of(ctx).pop(true),
                child: const Text('Post it'),
              ),
            ],
          ),
        ),
      );
      taxField.dispose();
      if (confirmed != true || !mounted) {
        setState(() => _posting = false);
        return;
      }
      await _ledger.createJournalEntry(
        buildJournalEntry(
          id: const Uuid().v4(),
          entryDate: DateTime.parse(line.date),
          description: posting.description,
          referenceNumber: posting.reference,
          lines: [
            for (final side in posting.sides)
              JournalLine(
                accountNumber: side.accountNumber,
                description: side.description,
                debit: side.debit,
                credit: side.credit,
              ),
          ],
        ),
      );
      if (!mounted) return;
      setState(() => _posting = false);
      _say('${posting.total.toDecimalString()} is in the books.');
    } catch (failure) {
      if (!mounted) return;
      setState(() => _posting = false);
      _say(CouldNotRead.reasonOf(failure));
    }
  }

  /// Record the money a payment took out of the bank.
  ///
  /// The purchases ledger knows the supplier was paid; it does not know the bank is emptier for it. The cash book is the
  /// record of what actually moved, and it is also where a money movement gets its ledger entry, so this hands the person
  /// that one step rather than writing a second, competing entry from here.
  Future<void> _recordTheMoneyLeaving(
    SupplierAccountLine line,
    String supplierName,
  ) async {
    final entry = await PaySupplierOutSheet.show(
      context,
      amount: line.amount,
      supplierName: supplierName,
      cash: _cash,
      ledger: _ledger,
      reference: line.reference.trim(),
    );
    if (!mounted) return;
    if (entry == null) return;
    _say(
      '${entry.amount.toDecimalString()} has left the bank. '
      'Its ledger entry is one tap on that payment in the cash book.',
    );
  }

  /// What was typed as tax, read without ever throwing at somebody mid-keystroke.
  ///
  /// An unfinished figure reads as nothing yet, so the sides above it simply show the bill as untaxed until the figure is
  /// whole. A figure the bill cannot carry is refused by `billPosting` and shown in place of the sides.
  ExactAmount _taxFrom(String typed) {
    final trimmed = typed.trim();
    if (trimmed.isEmpty) return ExactAmount.zero;
    try {
      return ExactAmount.parse(trimmed);
    } catch (_) {
      return ExactAmount.zero;
    }
  }

  void _say(String words) => ScaffoldMessenger.of(
    context,
  ).showSnackBar(SnackBar(content: Text(words)));

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(_account?.name ?? widget.supplierName ?? 'Supplier'),
      ),
      body: _body(),
    );
  }

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'This supplier account',
        error: _error,
        onRetry: _load,
      );
    }
    final account = _account;
    if (account == null || account.isEmpty) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(24),
          child: Text(
            'Nothing has passed between you and this supplier yet.',
            textAlign: TextAlign.center,
          ),
        ),
      );
    }
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Text(account.headline, style: Theme.of(context).textTheme.titleLarge),
        const SizedBox(height: 4),
        Text(
          '${account.billed.toDecimalString()} billed, '
          '${account.paid.toDecimalString()} paid'
          '${account.credited.isZero ? '' : ', ${account.credited.toDecimalString()} credited back'}, '
          'across ${account.lines.length} '
          '${account.lines.length == 1 ? 'movement' : 'movements'}.',
        ),
        const Divider(height: 32),
        // newest first, so the top line is the figure above
        for (final line in account.lines.reversed)
          _line(context, line, account.name),
        const Divider(height: 24),
        Row(
          children: [
            const Expanded(
              child: Text(
                'Owed now',
                style: TextStyle(fontWeight: FontWeight.bold),
              ),
            ),
            Text(
              account.balance.toDecimalString(),
              style: const TextStyle(fontWeight: FontWeight.bold),
            ),
          ],
        ),
        const SizedBox(height: 16),
        Text(
          'Newest first. The balance beside each line is what was owed just after it, so the '
          'top line is the figure above.',
          style: Theme.of(context).textTheme.bodySmall,
        ),
      ],
    );
  }

  Widget _line(
    BuildContext context,
    SupplierAccountLine line,
    String supplierName,
  ) {
    final adds = line.kind == 'purchase_invoice';
    // a bill and a payment are the two movements that belong in the accounts; a discount moves nothing to post
    final postable =
        line.moves &&
        (line.kind == 'purchase_invoice' || line.kind == 'payment');
    return InkWell(
      onTap: postable && !_posting
          ? () => _postToTheBooks(line, supplierName)
          : null,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 6),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(
              width: 84,
              child: Text(
                line.date,
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(line.wording),
                  if (line.reference.isNotEmpty)
                    Text(
                      line.reference,
                      style: Theme.of(context).textTheme.bodySmall,
                    ),
                  Text(
                    'balance ${line.running.toDecimalString()}',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                ],
              ),
            ),
            const SizedBox(width: 8),
            Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Text(
                  // a movement the ledger does not count is shown as it is, not dressed up as a payment
                  line.moves
                      ? (adds
                            ? line.amount.toDecimalString()
                            : '−${line.amount.toDecimalString()}')
                      : '—',
                  style: TextStyle(
                    fontWeight: FontWeight.w600,
                    color: line.moves && !adds ? Colors.green.shade800 : null,
                  ),
                ),
                if (postable)
                  Text(
                    'Post to books',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
