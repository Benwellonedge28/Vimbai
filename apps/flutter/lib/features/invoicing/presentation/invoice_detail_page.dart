// One invoice: what was billed, what has come in, and recording the next payment.
//
// Recording a payment against an invoice is not the same as posting it: the engine writes no journal entry, and says so in
// its own answer. That distinction is kept on the screen rather than smoothed over, because a Book whose invoices are all
// marked paid and whose income statement has never seen the money is exactly the mistake a business finds out about at
// year end.
import 'package:flutter/material.dart';
import 'package:uuid/uuid.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/models/journal_entry_drafting.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/tax/data/sale_tax.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/take_payment_in_sheet.dart';

class InvoiceDetailPage extends StatefulWidget {
  const InvoiceDetailPage({
    super.key,
    required this.invoice,
    this.service,
    this.accounting,
    this.cash,
  });

  final Invoice invoice;

  /// A test passes its own service so the page can be exercised without a server.
  final InvoicingService? service;

  /// The ledger this Book posts to, injectable for the same reason.
  final AccountingApiService? accounting;

  /// The cash book, for taking in the money a payment is about.
  final CashBookService? cash;

  @override
  State<InvoiceDetailPage> createState() => _InvoiceDetailPageState();
}

class _InvoiceDetailPageState extends State<InvoiceDetailPage> {
  late final InvoicingService _invoices =
      widget.service ?? InvoicingService.instance;

  late final AccountingApiService _ledger =
      widget.accounting ?? AccountingApiService();

  late final CashBookService _cash = widget.cash ?? CashBookService();

  late Invoice _invoice = widget.invoice;
  bool _recording = false;
  bool _posting = false;
  bool _posted = false;

  /// The payment just recorded, while it has not yet reached the cash book.
  ExactAmount? _toTakeIn;
  bool _taking = false;

  Future<void> _recordPayment() async {
    final amount = await showDialog<ExactAmount>(
      context: context,
      builder: (ctx) => _PaymentDialog(outstanding: _invoice.outstanding),
    );
    if (amount == null || !mounted) return;
    setState(() => _recording = true);
    try {
      final result = await _invoices.recordPayment(_invoice, amount);
      if (!mounted) return;
      setState(() {
        _recording = false;
        _toTakeIn = amount;
        _invoice = Invoice(
          invoiceNumber: _invoice.invoiceNumber,
          customerId: _invoice.customerId,
          invoiceDate: _invoice.invoiceDate,
          dueDate: _invoice.dueDate,
          total: _invoice.total,
          paid: result.paid,
          status: result.status,
          lines: _invoice.lines,
        );
      });
      _say(
        result.settled
            ? '${_invoice.invoiceNumber} is settled. '
                  'The sale itself posts in Journal entries.'
            : '${result.amount.toDecimalString()} recorded. '
                  '${result.outstanding.toDecimalString()} still to come in.',
      );
    } catch (failure) {
      if (!mounted) return;
      setState(() => _recording = false);
      _say(CouldNotRead.reasonOf(failure));
    }
  }

  /// Record the money the payment was made with.
  ///
  /// The payment lowered what is owed; the money still has to land somewhere. Nothing here guesses which account it reached,
  /// and nothing is recorded twice: the failing case is a Book whose invoices are all settled and whose cash book has never
  /// seen a penny.
  Future<void> _takeTheMoneyIn() async {
    final amount = _toTakeIn;
    if (amount == null) return;
    setState(() => _taking = true);
    final entry = await TakePaymentInSheet.show(
      context,
      amount: amount,
      invoiceNumber: _invoice.invoiceNumber,
      customer: _invoice.customerId,
      cash: _cash,
      ledger: _ledger,
    );
    if (!mounted) return;
    setState(() {
      _taking = false;
      if (entry != null) _toTakeIn = null;
    });
    if (entry != null) {
      _say(
        '${entry.amount.toDecimalString()} is in ${entry.description}. '
        'Its ledger entry is one tap on that receipt.',
      );
    }
  }

  /// Put the sale itself into the ledger.
  ///
  /// Recording a payment against an invoice is bookkeeping against the invoice; the money reaching the books is a journal
  /// entry, and the ledger has exactly one route into it. This walks that route with the person rather than instead of
  /// them: the two accounts are proposed from their own chart, shown as they will be posted, and nothing is written until
  /// they say so. The invoice number travels as the entry's reference, so trying twice cannot post the same sale twice -
  /// the engine refuses the second one by name.
  Future<void> _postTheSale() async {
    setState(() => _posting = true);
    try {
      final accounts = await _ledger.getAllAccounts();
      final owed = _account(
        accounts,
        (account) =>
            account.accountType == 'asset' &&
            (account.accountName.toLowerCase().contains('receivable') ||
                account.accountName.toLowerCase().contains('debtor')),
      );
      final earned = _account(
        accounts,
        (account) =>
            account.accountType == 'revenue' ||
            account.accountType == 'income' ||
            account.accountName.toLowerCase().contains('sales'),
      );
      if (owed == null || earned == null) {
        throw Exception(
          'This Book\'s chart has no ${owed == null ? 'Accounts Receivable' : 'Sales'} account, '
          'so the sale cannot be posted from here. Add one in Chart of accounts.',
        );
      }
      // the Book's own VAT account, found by name: a taxed sale puts its tax there rather than in income
      final taxAccount = _account(
        accounts,
        (account) =>
            account.accountName.toLowerCase().contains('vat') ||
            account.accountName.toLowerCase().contains('tax'),
      );
      final total = _invoice.total;
      // the tax this invoice carries, if any: typed by the person because it is their invoice, never derived from a rate
      final taxField = TextEditingController();
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (ctx) => AlertDialog(
          title: const Text('Post the sale'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Dr ${owed.accountNumber} ${owed.accountName}  ${total.toDecimalString()}',
              ),
              Text(
                'Cr ${earned.accountNumber} ${earned.accountName}  ${total.toDecimalString()}',
              ),
              if (taxAccount != null) ...[
                const SizedBox(height: 12),
                TextField(
                  controller: taxField,
                  keyboardType: const TextInputType.numberWithOptions(
                    decimal: true,
                  ),
                  decoration: InputDecoration(
                    labelText: 'Tax charged on this invoice',
                    helperText:
                        'Leave it empty if the invoice carries none. It will be '
                        'posted to ${taxAccount.accountNumber} ${taxAccount.accountName}.',
                  ),
                ),
              ],
              const SizedBox(height: 12),
              Text(
                'Reference ${_invoice.invoiceNumber}. Posted entries are corrected by reversing '
                'them, never edited.',
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
      );
      final typedTax = taxField.text.trim();
      taxField.dispose();
      if (confirmed != true || !mounted) {
        setState(() => _posting = false);
        return;
      }
      // the split is built before anything is sent, so a tax the invoice cannot support stops here with a sentence
      final split = saleSplit(
        owed: PostingAccount(
          accountNumber: owed.accountNumber,
          accountName: owed.accountName,
        ),
        earned: PostingAccount(
          accountNumber: earned.accountNumber,
          accountName: earned.accountName,
        ),
        taxAccount: PostingAccount(
          accountNumber: taxAccount?.accountNumber ?? '',
          accountName: taxAccount?.accountName ?? '',
        ),
        total: total,
        tax: typedTax.isEmpty ? ExactAmount.zero : ExactAmount.parse(typedTax),
      );
      await _ledger.createJournalEntry(
        buildJournalEntry(
          id: const Uuid().v4(),
          entryDate: DateTime.parse(_invoice.invoiceDate),
          description: 'Invoice ${_invoice.invoiceNumber}',
          referenceNumber: _invoice.invoiceNumber,
          lines: split.linesFor(_invoice.invoiceNumber),
        ),
      );
      if (!mounted) return;
      setState(() {
        _posting = false;
        _posted = true;
      });
      _say('${_invoice.invoiceNumber} is in the ledger.');
    } catch (failure) {
      if (!mounted) return;
      setState(() => _posting = false);
      _say(CouldNotRead.reasonOf(failure));
    }
  }

  /// The first account in the chart that answers to [wanted].
  Account? _account(List<Account> accounts, bool Function(Account) wanted) {
    for (final account in accounts) {
      if (wanted(account)) return account;
    }
    return null;
  }

  void _say(String words) => ScaffoldMessenger.of(
    context,
  ).showSnackBar(SnackBar(content: Text(words)));

  @override
  Widget build(BuildContext context) {
    final owing = _invoice.outstanding;
    return Scaffold(
      appBar: AppBar(title: Text(_invoice.invoiceNumber)),
      floatingActionButton: owing.isZero
          ? null
          : FloatingActionButton.extended(
              onPressed: _recording ? null : _recordPayment,
              icon: const Icon(Icons.payments_outlined),
              label: const Text('Record a payment'),
            ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            _invoice.status == 'paid'
                ? 'Settled'
                : '${owing.toDecimalString()} still to come in',
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: 4),
          Text('Raised ${_invoice.invoiceDate} · due ${_invoice.dueDate}'),
          const SizedBox(height: 16),
          _row('Total', _invoice.total.toDecimalString(), bold: true),
          _row('Paid', _invoice.paid.toDecimalString()),
          _row('Still owing', owing.toDecimalString(), bold: true),
          const Divider(height: 32),
          const Text('Lines', style: TextStyle(fontWeight: FontWeight.bold)),
          const SizedBox(height: 8),
          for (final line in _invoice.lines)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 4),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      '${line.description} · ${line.quantity.toDecimalString()} at '
                      '${line.unitPrice.toDecimalString()}',
                    ),
                  ),
                  Text(line.amount.toDecimalString()),
                ],
              ),
            ),
          const Divider(height: 32),
          ListTile(
            contentPadding: EdgeInsets.zero,
            leading: const Icon(Icons.menu_book_outlined),
            title: const Text('Post the sale to the ledger'),
            subtitle: Text(
              _posted
                  ? 'This sale is in the ledger under reference ${_invoice.invoiceNumber}.'
                  : 'Dr Accounts Receivable, Cr Sales, for ${_invoice.total.toDecimalString()}. '
                        'You will see the entry before it is posted.',
            ),
            trailing: _posted
                ? const Icon(Icons.check_circle_outline)
                : const Icon(Icons.chevron_right),
            enabled: !_posted && !_posting,
            onTap: _postTheSale,
          ),
          const SizedBox(height: 8),
          Text(
            'Recording a payment here is bookkeeping against the invoice. The money reaching the '
            'ledger is a journal entry, and this screen writes one through the same route as '
            'Accounting does.',
            style: Theme.of(context).textTheme.bodySmall,
          ),
          if (_toTakeIn != null) ...[
            const SizedBox(height: 8),
            Card(
              color: Theme.of(context).colorScheme.surfaceContainerHighest,
              child: ListTile(
                leading: const Icon(Icons.savings_outlined),
                title: Text(
                  '${_toTakeIn!.toDecimalString()} is not in the cash book yet',
                ),
                subtitle: const Text(
                  'The invoice knows it was paid. Record where the money went so the cash book '
                  'knows it too.',
                ),
                trailing: _taking
                    ? const SizedBox(
                        width: 18,
                        height: 18,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.chevron_right),
                onTap: _taking ? null : _takeTheMoneyIn,
              ),
            ),
          ],
        ],
      ),
    );
  }

  Widget _row(String label, String value, {bool bold = false}) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 2),
    child: Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [
        Text(
          label,
          style: TextStyle(fontWeight: bold ? FontWeight.bold : null),
        ),
        Text(
          value,
          style: TextStyle(fontWeight: bold ? FontWeight.bold : null),
        ),
      ],
    ),
  );
}

/// Asks for a payment, starting from what is still owing.
class _PaymentDialog extends StatefulWidget {
  const _PaymentDialog({required this.outstanding});

  final ExactAmount outstanding;

  @override
  State<_PaymentDialog> createState() => _PaymentDialogState();
}

class _PaymentDialogState extends State<_PaymentDialog> {
  late final TextEditingController _amount = TextEditingController(
    text: widget.outstanding.toDecimalString(),
  );
  String? _complaint;

  @override
  void dispose() {
    _amount.dispose();
    super.dispose();
  }

  void _give() {
    final typed = ExactAmount.tryParse(_amount.text.trim());
    if (typed == null) {
      setState(() => _complaint = 'That is not a figure.');
      return;
    }
    if (typed.isZero) {
      setState(() => _complaint = 'A payment has to be more than nothing.');
      return;
    }
    if (typed > widget.outstanding) {
      setState(
        () => _complaint =
            'That is more than the ${widget.outstanding.toDecimalString()} still owing.',
      );
      return;
    }
    Navigator.of(context).pop(typed);
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Record a payment'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          TextField(
            controller: _amount,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: InputDecoration(
              labelText: 'Amount received',
              helperText:
                  '${widget.outstanding.toDecimalString()} is still owing on this invoice',
            ),
          ),
          if (_complaint != null) ...[
            const SizedBox(height: 8),
            Text(
              _complaint!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ],
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Not now'),
        ),
        ElevatedButton(onPressed: _give, child: const Text('Record it')),
      ],
    );
  }
}
