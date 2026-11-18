// Adding the account the money actually sits in.
//
// The opening balance is not a convenience: the engine makes it the account's balance, so it is the one figure that says
// what was already there before this app started watching. It is asked for plainly rather than left at zero, because a
// bank account added with nothing in it makes every later figure wrong by exactly what it started with.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';

class NewCashAccountSheet extends StatefulWidget {
  const NewCashAccountSheet({super.key, required this.service});

  final CashBookService service;

  /// Show it, and answer the account that was made, or null if the person changed their mind.
  static Future<CashAccount?> show(
    BuildContext context,
    CashBookService service,
  ) => showModalBottomSheet<CashAccount>(
    context: context,
    isScrollControlled: true,
    builder: (_) => NewCashAccountSheet(service: service),
  );

  @override
  State<NewCashAccountSheet> createState() => _NewCashAccountSheetState();
}

class _NewCashAccountSheetState extends State<NewCashAccountSheet> {
  final _code = TextEditingController();
  final _name = TextEditingController();
  final _bank = TextEditingController();
  final _opening = TextEditingController(text: '0.00');
  String _type = 'bank';
  String? _complaint;
  bool _saving = false;

  @override
  void dispose() {
    _code.dispose();
    _name.dispose();
    _bank.dispose();
    _opening.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final opening = ExactAmount.tryParse(_opening.text.trim());
    if (opening == null) {
      setState(() => _complaint = 'The opening balance is not a figure.');
      return;
    }
    setState(() {
      _saving = true;
      _complaint = null;
    });
    try {
      final made = await widget.service.addAccount(
        code: _code.text,
        name: _name.text,
        type: _type,
        bankName: _bank.text,
        opening: opening,
      );
      if (!mounted) return;
      Navigator.of(context).pop(made);
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
    return Padding(
      padding: EdgeInsets.only(
        left: 16,
        right: 16,
        top: 16,
        bottom: MediaQuery.of(context).viewInsets.bottom + 16,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Add a cash or bank account',
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _code,
            decoration: const InputDecoration(
              labelText: 'Code',
              helperText: 'A short name the entries will point at, like CB-001',
            ),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _name,
            decoration: const InputDecoration(
              labelText: 'Name, like Main bank',
            ),
          ),
          const SizedBox(height: 8),
          SegmentedButton<String>(
            segments: const [
              ButtonSegment(value: 'bank', label: Text('Bank')),
              ButtonSegment(value: 'cash', label: Text('Cash')),
              ButtonSegment(value: 'savings', label: Text('Savings')),
            ],
            selected: {_type},
            onSelectionChanged: (chosen) =>
                setState(() => _type = chosen.first),
          ),
          const SizedBox(height: 8),
          if (_type == 'bank')
            TextField(
              controller: _bank,
              decoration: const InputDecoration(
                labelText: 'Bank, if you want it named',
              ),
            ),
          const SizedBox(height: 8),
          TextField(
            controller: _opening,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'Opening balance',
              helperText: 'What is already in this account today',
            ),
          ),
          if (_complaint != null) ...[
            const SizedBox(height: 8),
            Text(
              _complaint!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ],
          const SizedBox(height: 16),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              TextButton(
                onPressed: () => Navigator.of(context).pop(),
                child: const Text('Not now'),
              ),
              const SizedBox(width: 8),
              ElevatedButton(
                onPressed: _saving ? null : _save,
                child: Text(_saving ? 'Saving…' : 'Add it'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
