// What you owe: the money going out, from the same ledger the engine replays.
//
// The invoices side answers "who owes me"; this is the other half of a business's day, and it was the last piece of the
// Bookkeeping story the phone could not show at all. The page leads with the total owed, because that is the number a person
// came for, then each supplier largest first with the day they were last active, and ends with the engine's own check that
// the purchases control account agrees with those balances.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/payables/data/payables_service.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/supplier_account_page.dart';

class PayablesPage extends StatefulWidget {
  const PayablesPage({super.key, this.service, this.today});

  /// A test passes its own service so the page can be exercised without a server.
  final PayablesService? service;

  /// A test fixes today, so the day a movement is dated is not the day the test runs.
  final String? today;

  @override
  State<PayablesPage> createState() => _PayablesPageState();
}

class _PayablesPageState extends State<PayablesPage> {
  late final PayablesService _ledger =
      widget.service ?? PayablesService.instance;

  late final String _today =
      widget.today ?? DateTime.now().toIso8601String().substring(0, 10);

  bool _loading = true;
  String? _error;
  PayablesPosition? _position;
  PayablesCheck? _check;

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
      final position = await _ledger.position();
      PayablesCheck? check;
      try {
        check = await _ledger.check();
      } catch (_) {
        // the balances are the page; a check that could not be made is said so in words, not with a failed page
        check = null;
      }
      if (!mounted) return;
      setState(() {
        _position = position;
        _check = check;
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

  /// Their account: the history behind the balance on this page.
  Future<void> _openAccount(OwedToSupplier supplier) async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => SupplierAccountPage(
          supplierId: supplier.supplierId,
          supplierName: supplier.name,
        ),
      ),
    );
    // a movement may have been recorded while it was open, so the balances are read again rather than assumed
    if (mounted) await _load();
  }

  Future<void> _record({OwedToSupplier? onto}) async {
    final position = _position;
    final saved = await RecordSupplierMovementSheet.show(
      context,
      service: _ledger,
      today: _today,
      onto: onto,
      known: position?.suppliers ?? const [],
    );
    if (saved != null) await _load();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('What you owe')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _loading ? null : () => _record(),
        icon: const Icon(Icons.receipt),
        label: const Text('Record a bill or a payment'),
      ),
      body: _body(),
    );
  }

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(what: 'What you owe', error: _error, onRetry: _load);
    }
    final position = _position;
    if (position == null || position.isEmpty) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Icon(Icons.receipt_outlined, size: 48),
              const SizedBox(height: 12),
              Text(
                'You owe no supplier on this Book.',
                style: Theme.of(context).textTheme.titleMedium,
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 8),
              const Text(
                'Record a bill you have received, and it will be kept here until it is paid.',
                textAlign: TextAlign.center,
              ),
            ],
          ),
        ),
      );
    }
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 96),
        children: [
          Text(
            '${position.total.toDecimalString()} owed to '
            '${position.suppliers.length} '
            '${position.suppliers.length == 1 ? 'supplier' : 'suppliers'}',
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: 16),
          for (final supplier in position.suppliers)
            Card(
              margin: const EdgeInsets.only(bottom: 8),
              child: ListTile(
                // the name opens their account: what was billed, what was paid, what is left
                onTap: () => _openAccount(supplier),
                title: Text(supplier.name),
                subtitle: Text(
                  supplier.lastActivity.isEmpty
                      ? '${supplier.movements} movements'
                      : '${supplier.movements} movements, last on ${supplier.lastActivity}',
                ),
                trailing: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      supplier.balance.toDecimalString(),
                      style: const TextStyle(fontWeight: FontWeight.bold),
                    ),
                    IconButton(
                      tooltip: 'Record a bill, payment or note',
                      icon: const Icon(Icons.add_circle_outline),
                      onPressed: () => _record(onto: supplier),
                    ),
                  ],
                ),
              ),
            ),
          const SizedBox(height: 16),
          _checkLine(context),
        ],
      ),
    );
  }

  Widget _checkLine(BuildContext context) {
    final check = _check;
    if (check == null) {
      return Text(
        'The control account could not be checked just now. The balances above are still the ledger\'s own.',
        style: Theme.of(context).textTheme.bodySmall,
      );
    }
    if (check.reconciled) {
      return Text(
        'The purchases control account agrees: ${check.controlAccount.toDecimalString()} against '
        '${check.supplierTotal.toDecimalString()} owed across these suppliers.',
        style: Theme.of(context).textTheme.bodySmall,
      );
    }
    return Text(
      'The control account says ${check.controlAccount.toDecimalString()} and these suppliers '
      'come to ${check.supplierTotal.toDecimalString()}, a difference of '
      '${check.difference.toDecimalString()}. That is a gap between the ledger and the account it '
      'posts to, so one of them is missing an entry.',
      style: Theme.of(
        context,
      ).textTheme.bodySmall?.copyWith(color: Colors.orange.shade900),
    );
  }
}

/// Record a bill received or a payment made, against a supplier the Book already knows or a new one.
class RecordSupplierMovementSheet extends StatefulWidget {
  const RecordSupplierMovementSheet({
    super.key,
    required this.service,
    required this.today,
    required this.known,
    this.onto,
  });

  final PayablesService service;
  final String today;
  final List<OwedToSupplier> known;
  final OwedToSupplier? onto;

  static Future<OwedToSupplier?> show(
    BuildContext context, {
    required PayablesService service,
    required String today,
    required List<OwedToSupplier> known,
    OwedToSupplier? onto,
  }) => showModalBottomSheet<OwedToSupplier>(
    context: context,
    isScrollControlled: true,
    builder: (_) => RecordSupplierMovementSheet(
      service: service,
      today: today,
      known: known,
      onto: onto,
    ),
  );

  @override
  State<RecordSupplierMovementSheet> createState() =>
      _RecordSupplierMovementSheetState();
}

class _RecordSupplierMovementSheetState
    extends State<RecordSupplierMovementSheet> {
  late final TextEditingController _supplier = TextEditingController(
    text: widget.onto?.name ?? '',
  );
  late final TextEditingController _amount = TextEditingController();
  final TextEditingController _reference = TextEditingController();
  final TextEditingController _dueDate = TextEditingController();

  SupplierMovement _movement = SupplierMovement.bill;
  bool _saving = false;
  String? _problem;

  @override
  void initState() {
    super.initState();
    // a payment is the usual reason to open a supplier that is already owed money
    if (widget.onto != null) _movement = SupplierMovement.payment;
  }

  @override
  void dispose() {
    _supplier.dispose();
    _amount.dispose();
    _reference.dispose();
    _dueDate.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final amount = ExactAmount.tryParse(_amount.text.trim());
    if (amount == null) {
      setState(
        () => _problem = 'That is not an amount: write it as 480 or 480.50.',
      );
      return;
    }
    setState(() {
      _saving = true;
      _problem = null;
    });
    try {
      final saved = await widget.service.record(
        supplierId: widget.onto?.supplierId ?? _supplier.text.trim(),
        supplierName: _supplier.text.trim(),
        movement: _movement,
        amount: amount,
        outstandingBefore: widget.onto?.balance,
        date: widget.today,
        reference: _reference.text.trim(),
        dueDate: _dueDate.text.trim(),
      );
      if (!mounted) return;
      Navigator.of(context).pop(saved);
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _problem = CouldNotRead.reasonOf(failure);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final onto = widget.onto;
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
            onto == null ? 'Record what you owe' : 'Against ${onto.name}',
            style: Theme.of(context).textTheme.titleMedium,
          ),
          if (onto != null)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text('${onto.balance.toDecimalString()} owed today'),
            ),
          const SizedBox(height: 12),
          if (onto == null)
            TextField(
              controller: _supplier,
              decoration: const InputDecoration(
                labelText: 'Supplier',
                helperText:
                    'The name on the bill, so you recognise them next time',
              ),
            ),
          const SizedBox(height: 8),
          DropdownButtonFormField<SupplierMovement>(
            initialValue: _movement,
            decoration: const InputDecoration(labelText: 'What happened'),
            items: [
              for (final movement in SupplierMovement.values)
                DropdownMenuItem(value: movement, child: Text(movement.label)),
            ],
            onChanged: (chosen) {
              if (chosen != null) setState(() => _movement = chosen);
            },
          ),
          TextField(
            controller: _amount,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(labelText: 'How much'),
          ),
          TextField(
            controller: _reference,
            decoration: const InputDecoration(
              labelText: 'Their bill or payment number, if it has one',
            ),
          ),
          if (_movement == SupplierMovement.bill)
            TextField(
              controller: _dueDate,
              decoration: const InputDecoration(
                labelText: 'The day it must be paid, if they said',
                helperText:
                    'Write it as 2026-11-30. Leave it empty and the bill is '
                    'listed with no date rather than counted as due today.',
              ),
            ),
          if (_problem != null) ...[
            const SizedBox(height: 12),
            Text(
              _problem!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ],
          const SizedBox(height: 16),
          Row(
            children: [
              TextButton(
                onPressed: _saving ? null : () => Navigator.of(context).pop(),
                child: const Text('Not now'),
              ),
              const Spacer(),
              FilledButton(
                onPressed: _saving ? null : _save,
                child: Text(_saving ? 'Recording' : 'Record it'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
