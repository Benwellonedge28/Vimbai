// Stock bought on account: the purchase that puts inventory in the books and a supplier's name against it.
//
// A bill received and a credit purchase look alike on paper and are not the same entry: a bill puts a cost against payables,
// a purchase puts inventory against payables. The engine keeps them apart, so this screen asks for a purchase and says which
// two sides it will write before anything is posted. Money that has not moved is not a payment, so nothing here touches the
// cash book.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/banking/data/currencies.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';
import 'package:vimbai_mobile_client/features/payables/data/purchase_posting.dart';

class BuyStockPage extends StatefulWidget {
  const BuyStockPage({
    super.key,
    this.posting,
    this.payables,
    this.currencies,
    this.vendorId,
    this.vendorName,
  });

  /// A test passes its own services so the page can be exercised without a server.
  final PurchasePostingService? posting;
  final PayablesService? payables;
  final CurrencyService? currencies;

  /// The supplier, when the page is opened from their own account.
  final String? vendorId;
  final String? vendorName;

  @override
  State<BuyStockPage> createState() => _BuyStockPageState();
}

class _BuyStockPageState extends State<BuyStockPage> {
  late final PurchasePostingService _posting =
      widget.posting ?? PurchasePostingService.instance;
  late final PayablesService _payables =
      widget.payables ?? PayablesService.instance;
  late final CurrencyService _currencies =
      widget.currencies ?? CurrencyService.instance;

  final TextEditingController _order = TextEditingController();
  final TextEditingController _vendor = TextEditingController();
  final TextEditingController _total = TextEditingController();
  final TextEditingController _discount = TextEditingController();
  final TextEditingController _tax = TextEditingController();
  final TextEditingController _rate = TextEditingController();
  final TextEditingController _notes = TextEditingController();

  DateTime _bought = DateTime.now();
  DateTime? _due;

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
    _read();
  }

  @override
  void dispose() {
    for (final field in [
      _order,
      _vendor,
      _total,
      _discount,
      _tax,
      _rate,
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
      // the suppliers are what the engine already knows the Book owes; a supplier never paid before is typed instead
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
      // a Book whose currencies could not be read still buys in its own money, which is what an empty code means
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

  Future<void> _pick({required bool bought}) async {
    final picked = await showDatePicker(
      context: context,
      initialDate: bought ? _bought : (_due ?? _bought),
      firstDate: DateTime(2000),
      lastDate: DateTime(2100),
    );
    if (picked == null || !mounted) return;
    setState(() {
      if (bought) {
        _bought = picked;
      } else {
        _due = picked;
      }
    });
  }

  /// Everything that would stop the engine posting, said before it is asked.
  String? get _refusal {
    if (_vendor.text.trim().isEmpty) {
      return 'Say which supplier the stock came from, so the money owed is '
          'against a name rather than nobody.';
    }
    if (_order.text.trim().isEmpty) {
      return 'Give the supplier\'s own reference for this purchase, so the same '
          'one cannot be entered twice by accident.';
    }
    final total = _figure(_total);
    if (total == null || total.isZero || total.isNegative) {
      return 'Say what the purchase came to, which is the figure on the '
          'supplier\'s document.';
    }
    final discount = _figure(_discount);
    if (discount != null && discount > total) {
      return 'A discount bigger than the purchase would leave nothing to pay for '
          'the stock.';
    }
    if (_currency.isNotEmpty && (_figure(_rate)?.isZero ?? true)) {
      return 'Say what one $_currency is worth in this Book\'s money, so the '
          'stock is carried at a figure the books can use.';
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
        entryDate: _day(_bought),
        orderNumber: _order.text,
        vendorId: _vendor.text,
        total: _figure(_total)!,
        discount: _figure(_discount),
        tax: _figure(_tax),
        dueDate: _due == null ? '' : _day(_due!),
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
      appBar: AppBar(title: const Text('Stock bought on account')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            'The stock arrives and the supplier is owed. Nothing leaves the '
            'bank, so this is not a payment.',
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
              helperText: 'Who the stock came from, and who is now owed.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _order,
            decoration: const InputDecoration(
              labelText: 'Their reference',
              helperText: 'The supplier\'s own number on the document.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _total,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'What it came to',
              helperText:
                  'The figure on the document, in the money it was raised in.',
            ),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 8),
          Row(
            children: [
              Expanded(
                child: TextField(
                  controller: _discount,
                  keyboardType: const TextInputType.numberWithOptions(
                    decimal: true,
                  ),
                  decoration: const InputDecoration(
                    labelText: 'Discount (if any)',
                  ),
                  onChanged: (_) => setState(() {}),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: TextField(
                  controller: _tax,
                  keyboardType: const TextInputType.numberWithOptions(
                    decimal: true,
                  ),
                  decoration: const InputDecoration(labelText: 'Tax (if any)'),
                  onChanged: (_) => setState(() {}),
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          DropdownButtonFormField<String>(
            initialValue: _currency,
            decoration: const InputDecoration(
              labelText: 'Which money it was raised in',
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
                labelText: 'Rate on the day of the purchase',
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
            title: const Text('Bought on'),
            subtitle: Text(_day(_bought)),
            trailing: TextButton(
              onPressed: () => _pick(bought: true),
              child: const Text('Change'),
            ),
          ),
          ListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Payable by'),
            subtitle: Text(_due == null ? 'No date agreed' : _day(_due!)),
            trailing: TextButton(
              onPressed: () => _pick(bought: false),
              child: const Text('Change'),
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
                    'Stock, or inventory, goes up. Accounts payable goes up by '
                    'the same. No bank or cash account is touched.',
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
            child: Text(_saving ? 'Recording…' : 'Record the purchase'),
          ),
          const SizedBox(height: 24),
        ],
      ),
    );
  }
}
