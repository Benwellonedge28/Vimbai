// Goods back to a supplier: the purchase turned round, and what the business is owed back.
//
// A return is not a refund of money. The stock goes back, what the supplier is owed falls, and the cost that was taken in
// comes out again in a returns account. No bank or cash account is touched, so this screen never asks for one: the money
// arrives later or is netted off a bill, and recording that arrival is the cash book's business.
//
// The figure asked for is the gross credit the supplier gave, because that is what the document says. Tax comes out of that
// gross rather than being added to it, exactly as it did on the bill, so the reclaim is reversed for what was claimed.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/banking/data/currencies.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';
import 'package:vimbai_mobile_client/features/payables/data/purchase_return_posting.dart';

class ReturnToSupplierPage extends StatefulWidget {
  const ReturnToSupplierPage({
    super.key,
    this.posting,
    this.payables,
    this.currencies,
    this.vendorId,
    this.vendorName,
    this.originalOrder,
  });

  /// A test passes its own services so the page can be exercised without a server.
  final PurchaseReturnPostingService? posting;
  final PayablesService? payables;
  final CurrencyService? currencies;

  /// The supplier, when the page is opened from their own account.
  final String? vendorId;
  final String? vendorName;

  /// The purchase being sent back, when it is known.
  final String? originalOrder;

  @override
  State<ReturnToSupplierPage> createState() => _ReturnToSupplierPageState();
}

class _ReturnToSupplierPageState extends State<ReturnToSupplierPage> {
  late final PurchaseReturnPostingService _posting =
      widget.posting ?? PurchaseReturnPostingService.instance;
  late final PayablesService _payables =
      widget.payables ?? PayablesService.instance;
  late final CurrencyService _currencies =
      widget.currencies ?? CurrencyService.instance;

  final TextEditingController _reference = TextEditingController();
  final TextEditingController _vendor = TextEditingController();
  final TextEditingController _order = TextEditingController();
  final TextEditingController _total = TextEditingController();
  final TextEditingController _tax = TextEditingController();
  final TextEditingController _rate = TextEditingController();
  final TextEditingController _reason = TextEditingController();
  final TextEditingController _notes = TextEditingController();

  DateTime _returned = DateTime.now();

  /// Empty means this Book's own money, which is the ordinary case.
  String _currency = '';

  List<IsoCurrency> _held = const [];
  List<OwedToSupplier> _suppliers = const [];
  bool _reading = true;
  bool _saving = false;
  String? _complaint;
  String? _readError;

  @override
  void initState() {
    super.initState();
    _vendor.text = widget.vendorId ?? '';
    _order.text = widget.originalOrder ?? '';
    _read();
  }

  @override
  void dispose() {
    for (final field in [
      _reference,
      _vendor,
      _order,
      _total,
      _tax,
      _rate,
      _reason,
      _notes,
    ]) {
      field.dispose();
    }
    super.dispose();
  }

  Future<void> _read() async {
    setState(() {
      _reading = true;
      _readError = null;
    });
    try {
      // the suppliers are what the engine already knows the Book owes; one never traded with before is typed instead
      final position = await _payables.position();
      if (!mounted) return;
      setState(() {
        _suppliers = position.suppliers;
        _reading = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _suppliers = const [];
        _reading = false;
        _readError = CouldNotRead.reasonOf(failure);
      });
    }
    try {
      final held = await _currencies.read();
      if (!mounted) return;
      setState(() => _held = held.held);
    } catch (_) {
      // a Book whose currencies could not be read still takes a return in its own money, which is what an empty code means
    }
  }

  ExactAmount? _figure(TextEditingController field) {
    final typed = field.text.trim();
    if (typed.isEmpty) return null;
    try {
      return ExactAmount.parse(typed);
    } catch (_) {
      return null;
    }
  }

  static String _day(DateTime date) =>
      '${date.year.toString().padLeft(4, '0')}-'
      '${date.month.toString().padLeft(2, '0')}-'
      '${date.day.toString().padLeft(2, '0')}';

  Future<void> _pickDay() async {
    final picked = await showDatePicker(
      context: context,
      initialDate: _returned,
      firstDate: DateTime(2000),
      lastDate: DateTime(2100),
    );
    if (picked == null || !mounted) return;
    setState(() => _returned = picked);
  }

  /// Everything that would stop the engine posting, said before it is asked.
  String? get _refusal {
    if (_vendor.text.trim().isEmpty) {
      return 'Say which supplier the goods went back to, so what they owe back '
          'is against a name rather than nobody.';
    }
    if (_reference.text.trim().isEmpty) {
      return 'Give this return a reference of your own, so the same one cannot '
          'be entered twice by accident.';
    }
    final total = _figure(_total);
    if (total == null || total.isZero || total.isNegative) {
      return 'Say what the supplier is crediting back, which is the figure on '
          'their credit note.';
    }
    final tax = _figure(_tax);
    if (tax != null && tax > total) {
      return 'The tax cannot be bigger than the credit it came out of.';
    }
    if (_currency.isNotEmpty && (_figure(_rate)?.isZero ?? true)) {
      return 'Say what one $_currency is worth in this Book\'s money, so the '
          'cost comes back out at a figure the books can use.';
    }
    return null;
  }

  Future<void> _post() async {
    final refusal = _refusal;
    if (refusal != null) {
      setState(() => _complaint = refusal);
      return;
    }
    setState(() {
      _saving = true;
      _complaint = null;
    });
    try {
      final posted = await _posting.post(
        entryDate: _day(_returned),
        returnNumber: _reference.text,
        vendorId: _vendor.text,
        total: _figure(_total)!,
        tax: _figure(_tax),
        originalOrder: _order.text,
        reason: _reason.text,
        currency: _currency,
        rate: _currency.isEmpty ? '' : _rate.text,
        notes: _notes.text,
      );
      if (!mounted) return;
      Navigator.of(context).pop(posted);
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _complaint = CouldNotRead.reasonOf(failure);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Goods back to a supplier')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            'The stock goes back and the supplier owes you what it credits. No '
            'money arrives with this, so nothing is recorded against the bank.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 16),
          if (_reading) const LinearProgressIndicator(),
          if (_readError != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Text(
                'The suppliers could not be read, so type the supplier\'s id '
                'instead: $_readError',
                style: theme.textTheme.bodySmall,
              ),
            ),
          if (_suppliers.isNotEmpty)
            DropdownButtonFormField<String>(
              initialValue:
                  _suppliers.any((one) => one.supplierId == _vendor.text)
                  ? _vendor.text
                  : null,
              decoration: const InputDecoration(
                labelText: 'Supplier already on the books',
              ),
              items: [
                for (final supplier in _suppliers)
                  DropdownMenuItem(
                    value: supplier.supplierId,
                    child: Text(
                      '${supplier.name} · ${supplier.balance.toDecimalString()} owed',
                    ),
                  ),
              ],
              onChanged: (picked) =>
                  setState(() => _vendor.text = picked ?? ''),
            ),
          if (_suppliers.isNotEmpty) const SizedBox(height: 8),
          TextField(
            controller: _vendor,
            decoration: const InputDecoration(
              labelText: 'Supplier',
              helperText: 'Who the goods went back to, and who owes you.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _reference,
            decoration: const InputDecoration(
              labelText: 'Your reference for this return',
              helperText: 'So the same return cannot be recorded twice.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _order,
            decoration: const InputDecoration(
              labelText: 'The purchase it reverses (if known)',
              helperText: 'The supplier\'s own number on the purchase.',
            ),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _total,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'What they are crediting back',
              helperText:
                  'The gross figure on their credit note, in the money it was '
                  'raised in.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _tax,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'Tax inside that figure (if any)',
              helperText:
                  'Comes out of the credit rather than being added to it, so '
                  'the reclaim is reversed for what was claimed.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          DropdownButtonFormField<String>(
            initialValue: _currency,
            decoration: const InputDecoration(
              labelText: 'Which money the credit is in',
            ),
            items: [
              const DropdownMenuItem(
                value: '',
                child: Text('This Book\'s own money'),
              ),
              for (final currency in _held)
                DropdownMenuItem(
                  value: currency.code,
                  child: Text('${currency.code} · ${currency.name}'),
                ),
            ],
            onChanged: (picked) => setState(() => _currency = picked ?? ''),
          ),
          if (_currency.isNotEmpty) ...[
            const SizedBox(height: 8),
            TextField(
              controller: _rate,
              keyboardType: const TextInputType.numberWithOptions(
                decimal: true,
              ),
              decoration: InputDecoration(
                labelText: 'Rate on the day of the return',
                helperText:
                    'How much one $_currency is worth in this Book\'s money. '
                    'The engine converts at this rate and keeps what the '
                    'supplier showed.',
              ),
              onChanged: (_) => setState(() {}),
            ),
          ],
          const SizedBox(height: 8),
          ListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Sent back on'),
            subtitle: Text(_day(_returned)),
            trailing: TextButton(
              onPressed: _pickDay,
              child: const Text('Change'),
            ),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _reason,
            decoration: const InputDecoration(
              labelText: 'Why it went back (if any)',
              helperText: 'Damaged, wrong item, short delivery, and so on.',
            ),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _notes,
            decoration: const InputDecoration(labelText: 'Notes (if any)'),
          ),
          const SizedBox(height: 16),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'What this will write',
                    style: theme.textTheme.titleSmall,
                  ),
                  const SizedBox(height: 4),
                  Text(
                    'Accounts payable goes down by what the supplier credits '
                    'back, and purchases returns is credited by the same, so '
                    'the cost taken in comes back out. No bank or cash account '
                    'is touched.',
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
            ),
          ),
          if (_complaint != null) ...[
            const SizedBox(height: 12),
            Text(
              _complaint!,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.error,
              ),
            ),
          ],
          const SizedBox(height: 16),
          FilledButton(
            onPressed: _saving ? null : _post,
            child: Text(_saving ? 'Recording…' : 'Record the return'),
          ),
          const SizedBox(height: 24),
        ],
      ),
    );
  }
}
