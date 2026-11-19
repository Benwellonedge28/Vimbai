// Taking a sale back, from the invoice it came from.
//
// The invoice already knows the customer, the number and the total, so nothing here asks a person to retype them. What it
// does ask is which taxes the sale carried, because only the person knows whether this particular invoice had tax on it,
// and the engine needs that to take each one back out at the rate it was charged at.
//
// Nothing is posted until the person says so, and the figures shown before they do are the engine's own, not this app's
// arithmetic: the same preview the sale screen uses, read the other way round.
import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/sales_return.dart';
import 'package:vimbai_mobile_client/features/tax/data/sale_tax.dart';
import 'package:vimbai_mobile_client/features/tax/data/tax_table.dart';

/// Open the sheet. Answers true when a return was posted.
Future<bool?> showTakeASaleBack(
  BuildContext context, {
  required Invoice invoice,
  SaleTaxReads? tax,
  SalesReturnService? returns,
}) => showModalBottomSheet<bool>(
  context: context,
  isScrollControlled: true,
  builder: (_) => TakeASaleBackSheet(
    invoice: invoice,
    tax: tax ?? EngineSaleTax(),
    returns: returns ?? SalesReturnService.instance,
  ),
);

class TakeASaleBackSheet extends StatefulWidget {
  const TakeASaleBackSheet({
    super.key,
    required this.invoice,
    required this.tax,
    required this.returns,
  });

  final Invoice invoice;
  final SaleTaxReads tax;
  final SalesReturnService returns;

  @override
  State<TakeASaleBackSheet> createState() => _TakeASaleBackSheetState();
}

class _TakeASaleBackSheetState extends State<TakeASaleBackSheet> {
  late final TextEditingController _number = TextEditingController(
    text: 'CN-${widget.invoice.invoiceNumber}',
  );
  final TextEditingController _reason = TextEditingController();

  List<BookTax> _onSales = const [];
  final Set<String> _chosen = {};
  TaxPreview? _carried;
  String _taxTrouble = '';
  bool _reading = true;
  bool _posting = false;
  String _trouble = '';

  @override
  void initState() {
    super.initState();
    _readTheTaxes();
  }

  @override
  void dispose() {
    _number.dispose();
    _reason.dispose();
    super.dispose();
  }

  /// The taxes this Book charges on a sale on the invoice's own day, all of them chosen to begin with: a return of the
  /// whole invoice takes back the whole of what it carried.
  Future<void> _readTheTaxes() async {
    try {
      final onSales = await widget.tax.chargedOnSales(
        widget.invoice.invoiceDate,
      );
      if (!mounted) return;
      setState(() {
        _onSales = onSales;
        _chosen.addAll(onSales.map((one) => one.name));
        _reading = false;
      });
      await _preview();
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _reading = false;
        _taxTrouble = CouldNotRead.reasonOf(failure);
      });
    }
  }

  Future<void> _preview() async {
    if (_chosen.isEmpty) {
      setState(() => _carried = null);
      return;
    }
    try {
      final answer = await widget.tax.preview(
        date: widget.invoice.invoiceDate,
        gross: widget.invoice.total.toDecimalString(),
        taxes: _chosen.toList(),
      );
      if (!mounted) return;
      setState(() {
        _carried = answer;
        _taxTrouble = '';
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() => _taxTrouble = CouldNotRead.reasonOf(failure));
    }
  }

  Future<void> _post() async {
    final number = _number.text.trim();
    if (number.isEmpty) {
      setState(() => _trouble = 'A return needs a number of its own.');
      return;
    }
    setState(() {
      _posting = true;
      _trouble = '';
    });
    try {
      final posted = await widget.returns.record(
        entryDate: widget.invoice.invoiceDate,
        returnNumber: number,
        originalInvoiceNumber: widget.invoice.invoiceNumber,
        customerId: widget.invoice.customerId,
        total: widget.invoice.total,
        taxes: _chosen.toList(),
        reason: _reason.text,
      );
      if (!mounted) return;
      Navigator.of(context).pop(true);
      final tax = ExactAmount.fromJson(posted.taxAmount);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            '${posted.returnNumber} posted. '
            '${posted.totalAmount} credited'
            '${tax.isZero ? '' : ', of which $tax was tax'}',
          ),
        ),
      );
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _posting = false;
        _trouble = CouldNotRead.reasonOf(failure);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final total = widget.invoice.total;
    final carried = _carried;
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
              'Take this sale back',
              style: Theme.of(context).textTheme.titleLarge,
            ),
            const SizedBox(height: 8),
            Text(
              'The customer is credited ${total.toDecimalString()} in full. Each tax the sale '
              'carried comes back off the expense it was charged to and off what you owe, so the '
              'books never keep tax on a sale that did not happen.',
            ),
            const SizedBox(height: 16),
            TextField(
              controller: _number,
              decoration: const InputDecoration(
                labelText: 'Return number',
                helperText: 'What this credit note is called',
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _reason,
              decoration: const InputDecoration(
                labelText: 'Why it came back',
                helperText: 'Optional',
              ),
            ),
            const SizedBox(height: 16),
            if (_reading)
              const Padding(
                padding: EdgeInsets.symmetric(vertical: 8),
                child: LinearProgressIndicator(),
              )
            else if (_taxTrouble.isNotEmpty)
              Text(
                _taxTrouble,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              )
            else if (_onSales.isEmpty)
              const Text(
                'This Book charges no tax on a sale, so none comes back.',
              )
            else ...[
              const Text(
                'Which taxes did this sale carry?',
                style: TextStyle(fontWeight: FontWeight.bold),
              ),
              for (final one in _onSales)
                CheckboxListTile(
                  contentPadding: EdgeInsets.zero,
                  value: _chosen.contains(one.name),
                  title: Text(one.name),
                  subtitle: Text(one.rateText),
                  onChanged: (picked) async {
                    setState(() {
                      if (picked == true) {
                        _chosen.add(one.name);
                      } else {
                        _chosen.remove(one.name);
                      }
                    });
                    await _preview();
                  },
                ),
              if (carried != null && !carried.carriesNothing)
                Card(
                  color: Theme.of(context).colorScheme.surfaceContainerHighest,
                  child: Padding(
                    padding: const EdgeInsets.all(12),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          'Credited ${carried.gross.toDecimalString()}, of which '
                          '${carried.tax.toDecimalString()} is tax',
                          style: const TextStyle(fontWeight: FontWeight.bold),
                        ),
                        const SizedBox(height: 4),
                        for (final line in carried.lines)
                          if (!line.amount.isZero)
                            Text(
                              '${line.tax}: ${line.amount.toDecimalString()} back off '
                              '${line.expenseAccount}',
                            ),
                      ],
                    ),
                  ),
                ),
            ],
            if (_trouble.isNotEmpty) ...[
              const SizedBox(height: 12),
              Text(
                _trouble,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
            const SizedBox(height: 16),
            Row(
              children: [
                TextButton(
                  onPressed: _posting
                      ? null
                      : () => Navigator.of(context).pop(),
                  child: const Text('Not now'),
                ),
                const Spacer(),
                FilledButton.icon(
                  onPressed: _posting || _reading ? null : _post,
                  icon: _posting
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.undo),
                  label: const Text('Post the return'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
