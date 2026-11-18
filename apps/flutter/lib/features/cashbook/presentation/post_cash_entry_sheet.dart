// Putting one receipt or payment into the ledger.
//
// The cash book moves an account's balance; the ledger is where the double entry lives, and the two are not the same record.
// This sheet writes the entry a receipt or payment actually is - money in: Dr the account it landed in, Cr what it was for;
// money out: Dr what it was spent on, Cr the account it left - using the account already named on the cash book entry and the
// ledger account the person says the money sits in.
//
// Nothing is guessed silently. Both sides are proposed from the Book's own chart and both are shown before anything is
// written, because the ledger is the record the statements come from and a wrong side there is wrong everywhere.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/models/journal_entry_drafting.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:uuid/uuid.dart';

/// The reference an entry carries into the ledger, which is what stops it being posted twice.
///
/// It is the cash book's own id for the entry, so the two records point at each other and the engine refuses a second
/// posting of the same money by name rather than silently doubling it.
String cashEntryReference(CashEntry entry) => 'CB-${entry.id}';

class PostCashEntrySheet extends StatefulWidget {
  const PostCashEntrySheet({
    super.key,
    required this.entry,
    required this.account,
    required this.ledger,
  });

  final CashEntry entry;
  final CashAccount account;
  final AccountingApiService ledger;

  static Future<bool?> show(
    BuildContext context, {
    required CashEntry entry,
    required CashAccount account,
    required AccountingApiService ledger,
  }) => showModalBottomSheet<bool>(
    context: context,
    isScrollControlled: true,
    builder: (_) =>
        PostCashEntrySheet(entry: entry, account: account, ledger: ledger),
  );

  @override
  State<PostCashEntrySheet> createState() => _PostCashEntrySheetState();
}

class _PostCashEntrySheetState extends State<PostCashEntrySheet> {
  bool _loading = true;
  String? _error;
  String? _complaint;
  bool _posting = false;
  List<Account> _chart = const [];
  Account? _moneySide;
  Account? _otherSide;

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
      final chart = await widget.ledger.getAllAccounts();
      if (!mounted) return;
      setState(() {
        _chart = chart;
        _moneySide = _proposeMoneySide(chart);
        _otherSide = _proposeOtherSide(chart);
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

  /// The ledger account the money itself sits in: an asset named after this cash account, else one that says cash or bank.
  Account? _proposeMoneySide(List<Account> chart) {
    final name = widget.account.name.toLowerCase();
    Account? byName;
    Account? byWord;
    for (final account in chart) {
      final theirs = account.accountName.toLowerCase();
      final asset = account.accountType == 'asset';
      if (asset && name.isNotEmpty && theirs.contains(name)) byName ??= account;
      if (asset && (theirs.contains('cash') || theirs.contains('bank'))) {
        byWord ??= account;
      }
    }
    return byName ?? byWord;
  }

  /// The other side, which the cash book entry already names.
  Account? _proposeOtherSide(List<Account> chart) {
    final named = widget.entry.againstAccount.trim();
    if (named.isEmpty) return null;
    for (final account in chart) {
      if (account.accountNumber == named) return account;
    }
    return null;
  }

  Future<void> _post() async {
    final moneySide = _moneySide;
    final otherSide = _otherSide;
    if (moneySide == null || otherSide == null) {
      setState(
        () => _complaint =
            'Choose both accounts: the one the money sits in, and the one this entry is against.',
      );
      return;
    }
    if (moneySide.accountNumber == otherSide.accountNumber) {
      setState(
        () => _complaint =
            'Both sides are ${moneySide.accountNumber}. An entry has to move money between two accounts.',
      );
      return;
    }
    final amount = widget.entry.amount;
    final incoming = widget.entry.moneyIn;
    setState(() {
      _posting = true;
      _complaint = null;
    });
    try {
      await widget.ledger.createJournalEntry(
        buildJournalEntry(
          id: const Uuid().v4(),
          entryDate: DateTime.parse(widget.entry.date),
          description: widget.entry.description.isEmpty
              ? (incoming ? 'Money in' : 'Money out')
              : widget.entry.description,
          referenceNumber: cashEntryReference(widget.entry),
          lines: [
            JournalLine(
              accountNumber: incoming
                  ? moneySide.accountNumber
                  : otherSide.accountNumber,
              description: widget.entry.description,
              debit: amount,
            ),
            JournalLine(
              accountNumber: incoming
                  ? otherSide.accountNumber
                  : moneySide.accountNumber,
              description: widget.entry.description,
              credit: amount,
            ),
          ],
        ),
      );
      if (!mounted) return;
      Navigator.of(context).pop(true);
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _posting = false;
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
      child: SingleChildScrollView(child: _body()),
    );
  }

  Widget _body() {
    final entry = widget.entry;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Post ${entry.description.isEmpty ? 'this entry' : entry.description} to the ledger',
          style: Theme.of(context).textTheme.titleLarge,
        ),
        const SizedBox(height: 4),
        Text(
          '${entry.moneyIn ? 'Money in' : 'Money out'} of '
          '${entry.amount.toDecimalString()} on ${entry.date}',
        ),
        const SizedBox(height: 12),
        if (_loading) const LinearProgressIndicator(),
        if (_error != null)
          CouldNotRead(
            what: 'The Book\'s chart of accounts',
            error: _error,
            onRetry: _load,
          ),
        if (!_loading && _error == null) ...[
          _picker(
            label: 'The account the money sits in',
            value: _moneySide,
            onChosen: (account) => setState(() => _moneySide = account),
          ),
          const SizedBox(height: 8),
          _picker(
            label: entry.moneyIn
                ? 'What it came in for'
                : 'What it was spent on',
            value: _otherSide,
            onChosen: (account) => setState(() => _otherSide = account),
          ),
          const SizedBox(height: 12),
          if (_moneySide != null && _otherSide != null) ...[
            Text(
              entry.moneyIn
                  ? 'Dr ${_moneySide!.accountNumber} ${_moneySide!.accountName}  '
                        '${entry.amount.toDecimalString()}'
                  : 'Dr ${_otherSide!.accountNumber} ${_otherSide!.accountName}  '
                        '${entry.amount.toDecimalString()}',
              style: const TextStyle(fontFamily: 'monospace'),
            ),
            Text(
              entry.moneyIn
                  ? 'Cr ${_otherSide!.accountNumber} ${_otherSide!.accountName}  '
                        '${entry.amount.toDecimalString()}'
                  : 'Cr ${_moneySide!.accountNumber} ${_moneySide!.accountName}  '
                        '${entry.amount.toDecimalString()}',
              style: const TextStyle(fontFamily: 'monospace'),
            ),
            const SizedBox(height: 8),
            Text(
              'Reference ${cashEntryReference(entry)}. A posted entry is corrected by a '
              'reversing entry, never edited.',
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
              onPressed: _posting || _loading ? null : _post,
              child: Text(_posting ? 'Posting…' : 'Post it'),
            ),
          ],
        ),
      ],
    );
  }

  Widget _picker({
    required String label,
    required Account? value,
    required ValueChanged<Account?> onChosen,
  }) {
    final chosen = value;
    return DropdownButtonFormField<String>(
      initialValue: chosen?.accountNumber,
      isExpanded: true,
      decoration: InputDecoration(
        labelText: label,
        helperText: chosen == null
            ? 'Not suggested by the chart - choose one'
            : null,
      ),
      items: [
        for (final account in _chart)
          DropdownMenuItem(
            value: account.accountNumber,
            child: Text(
              '${account.accountNumber} · ${account.accountName}',
              overflow: TextOverflow.ellipsis,
            ),
          ),
      ],
      onChanged: (number) {
        for (final account in _chart) {
          if (account.accountNumber == number) {
            onChosen(account);
            return;
          }
        }
      },
    );
  }
}
