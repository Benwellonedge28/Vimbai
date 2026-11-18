// Checking the cash book against the statement the bank actually sent.
//
// This is the one place the books are compared with somebody outside them, so it is the one place a mistake in the cash book
// has a chance of being noticed. The engine holds both figures and the arithmetic; what this sheet does is ask for the
// statement balance and then say plainly whether the two agree, and if they do not, what the gap is and the three ordinary
// reasons for one.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';

class ReconcileSheet extends StatefulWidget {
  const ReconcileSheet({
    super.key,
    required this.account,
    required this.service,
  });

  final CashAccount account;
  final CashBookService service;

  static Future<CashReconciliation?> show(
    BuildContext context, {
    required CashAccount account,
    required CashBookService service,
  }) => showModalBottomSheet<CashReconciliation>(
    context: context,
    isScrollControlled: true,
    builder: (_) => ReconcileSheet(account: account, service: service),
  );

  @override
  State<ReconcileSheet> createState() => _ReconcileSheetState();
}

class _ReconcileSheetState extends State<ReconcileSheet> {
  final _statement = TextEditingController();
  final _date = TextEditingController(
    text: DateTime.now().toIso8601String().substring(0, 10),
  );
  String? _complaint;
  bool _saving = false;

  @override
  void dispose() {
    _statement.dispose();
    _date.dispose();
    super.dispose();
  }

  ExactAmount? get _entered => ExactAmount.tryParse(_statement.text.trim());

  Future<void> _save() async {
    final statement = _entered;
    if (statement == null) {
      setState(() => _complaint = 'That is not a figure.');
      return;
    }
    setState(() {
      _saving = true;
      _complaint = null;
    });
    try {
      final made = await widget.service.reconcile(
        account: widget.account,
        statementDate: _date.text,
        statementBalance: statement,
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
    final books = widget.account.balance;
    final statement = _entered;
    final gap = statement == null ? null : books - statement;
    return Padding(
      padding: EdgeInsets.only(
        left: 16,
        right: 16,
        top: 16,
        bottom: MediaQuery.of(context).viewInsets.bottom + 16,
      ),
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Reconcile ${widget.account.name}',
              style: Theme.of(context).textTheme.titleLarge,
            ),
            const SizedBox(height: 4),
            Text(
              'Your books say ${books.toDecimalString()} in this account. '
              'Enter what the bank statement says on its closing date.',
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _date,
              decoration: const InputDecoration(
                labelText: 'Date of the statement',
              ),
            ),
            const SizedBox(height: 8),
            TextField(
              controller: _statement,
              keyboardType: const TextInputType.numberWithOptions(
                decimal: true,
              ),
              onChanged: (_) => setState(() {}),
              decoration: const InputDecoration(
                labelText: 'Statement balance',
                helperText: 'The closing balance printed by the bank',
              ),
            ),
            if (gap != null) ...[
              const SizedBox(height: 12),
              if (gap.isZero)
                Text(
                  'They agree. Nothing to chase.',
                  style: TextStyle(
                    color: Colors.green.shade800,
                    fontWeight: FontWeight.bold,
                  ),
                )
              else ...[
                Text(
                  gap.isNegative
                      ? 'The bank shows ${gap.negated().toDecimalString()} more than your books'
                      : 'Your books show ${gap.toDecimalString()} more than the bank',
                  style: const TextStyle(fontWeight: FontWeight.bold),
                ),
                const SizedBox(height: 4),
                Text(
                  'That gap is the work: cheques you have written that have not cleared, '
                  'money paid in that has not reached the statement, or a bank charge and its '
                  'entry that both still have to be made. Recording it now keeps the check as '
                  'it stood today, gap and all.',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ],
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
                  child: Text(_saving ? 'Recording…' : 'Record the check'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// Says whether the books and the bank agreed on a recorded check, in one line.
String reconciliationLine(CashReconciliation check) {
  final where = check.agrees
      ? 'agreed'
      : check.gap.isNegative
      ? 'bank ${check.gap.negated().toDecimalString()} ahead'
      : 'books ${check.gap.toDecimalString()} ahead';
  return '${check.statementDate} · bank ${check.statementBalance.toDecimalString()} · '
      'books ${check.bookBalance.toDecimalString()} · $where';
}
