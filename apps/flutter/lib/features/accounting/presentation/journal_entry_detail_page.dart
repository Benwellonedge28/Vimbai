import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';

class JournalEntryDetailPage extends StatefulWidget {
  final JournalEntry entry;

  /// The engine the page talks to. A test hands it one that answers without a phone.
  final AccountingApiService? service;

  const JournalEntryDetailPage({super.key, required this.entry, this.service});

  @override
  State<JournalEntryDetailPage> createState() => _JournalEntryDetailPageState();
}

class _JournalEntryDetailPageState extends State<JournalEntryDetailPage> {
  late final AccountingApiService _apiService =
      widget.service ?? AccountingApiService();

  /// The correction this page has written, once it has one. Kept so the door closes behind it: the engine refuses a
  /// second correction of the same entry, and offering the button again would only invite that refusal.
  String? _correctionId;

  /// Whether a correction is being written, so the button cannot be pressed twice while the first is in flight.
  bool _reversing = false;

  /// A posted entry is corrected, never edited, so this is the door for a mistake. A note is a different matter: it is
  /// edited or thrown away, and the engine refuses to reverse one, so the button is not offered for it.
  bool get _canReverse => widget.entry.status == 'posted' && _correctionId == null;

  Future<void> _reverse() async {
    final agreed = await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: const Text('Correct this entry?'),
        content: Text(
          'A second entry will be written that says the opposite of "${widget.entry.description}", and the books will '
          'come back to where they were. The original stays, because a posted entry is never removed or changed, and '
          'the correction will point back at it.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialog).pop(false),
            child: const Text('Keep it as it is'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialog).pop(true),
            child: const Text('Write the correction'),
          ),
        ],
      ),
    );
    if (agreed != true || !mounted) return;

    setState(() => _reversing = true);
    final messenger = ScaffoldMessenger.of(context);
    try {
      final correction = await _apiService.reverseJournalEntry(widget.entry.id);
      if (!mounted) return;
      setState(() {
        _correctionId = correction.id;
        _reversing = false;
      });
      messenger.showSnackBar(
        SnackBar(
          content: Text('The correction was written against "${widget.entry.description}".'),
        ),
      );
    } catch (error) {
      if (!mounted) return;
      setState(() => _reversing = false);
      // the engine's own words, because its refusal says why: already corrected, or not a posting at all
      messenger.showSnackBar(
        SnackBar(
          content: Text(
            'Could not correct it: ${error.toString().replaceFirst('Exception: ', '')}',
          ),
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final entry = widget.entry;
    final totalDebit = ExactAmount.sum(entry.lines.map((line) => line.debit));
    final totalCredit = ExactAmount.sum(entry.lines.map((line) => line.credit));

    return Scaffold(
      appBar: AppBar(title: const Text('Journal Entry Details')),
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Card(
              child: Padding(
                padding: const EdgeInsets.all(16.0),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      entry.description,
                      style: const TextStyle(
                        fontSize: 18,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    const SizedBox(height: 12),
                    Row(
                      children: [
                        const Icon(Icons.calendar_today, size: 16),
                        const SizedBox(width: 8),
                        Text(
                          'Date: ${DateFormat.yMMMd().format(entry.entryDate)}',
                        ),
                      ],
                    ),
                    const SizedBox(height: 4),
                    Row(
                      children: [
                        const Icon(Icons.tag, size: 16),
                        const SizedBox(width: 8),
                        Text('Reference: ${entry.referenceNumber ?? "N/A"}'),
                      ],
                    ),
                    const SizedBox(height: 4),
                    Row(
                      children: [
                        const Icon(Icons.source, size: 16),
                        const SizedBox(width: 8),
                        Text('Source: ${entry.sourceModule}'),
                      ],
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 20),
            const Text(
              'Journal Lines',
              style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
            ),
            const SizedBox(height: 10),
            if (entry.lines.isEmpty)
              const Text('No lines found.')
            else
              ...entry.lines.asMap().entries.map((entryMap) {
                int idx = entryMap.key;
                JournalLine line = entryMap.value;
                return Card(
                  margin: const EdgeInsets.symmetric(vertical: 4),
                  child: Padding(
                    padding: const EdgeInsets.all(12.0),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          'Line ${idx + 1}',
                          style: const TextStyle(
                            fontWeight: FontWeight.bold,
                            color: Colors.grey,
                          ),
                        ),
                        const SizedBox(height: 8),
                        Row(
                          children: [
                            const Icon(Icons.account_balance, size: 16),
                            const SizedBox(width: 8),
                            Expanded(
                              child: Text(
                                line.accountNumber,
                                style: const TextStyle(
                                  fontWeight: FontWeight.bold,
                                ),
                              ),
                            ),
                          ],
                        ),
                        const SizedBox(height: 4),
                        if (line.description != null &&
                            line.description!.isNotEmpty) ...[
                          Row(
                            children: [
                              const Icon(Icons.description, size: 16),
                              const SizedBox(width: 8),
                              Expanded(child: Text(line.description!)),
                            ],
                          ),
                          const SizedBox(height: 4),
                        ],
                        const Divider(),
                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                const Text(
                                  'Debit',
                                  style: TextStyle(
                                    fontSize: 12,
                                    color: Colors.grey,
                                  ),
                                ),
                                Text(
                                  '\$${line.debit.toDecimalString()}',
                                  style: const TextStyle(
                                    fontSize: 16,
                                    fontWeight: FontWeight.bold,
                                    color: Colors.red,
                                  ),
                                ),
                              ],
                            ),
                            Column(
                              crossAxisAlignment: CrossAxisAlignment.end,
                              children: [
                                const Text(
                                  'Credit',
                                  style: TextStyle(
                                    fontSize: 12,
                                    color: Colors.grey,
                                  ),
                                ),
                                Text(
                                  '\$${line.credit.toDecimalString()}',
                                  style: const TextStyle(
                                    fontSize: 16,
                                    fontWeight: FontWeight.bold,
                                    color: Colors.green,
                                  ),
                                ),
                              ],
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                );
              }).toList(),
            const SizedBox(height: 16),
            Card(
              color: totalDebit == totalCredit
                  ? Colors.green.shade50
                  : Colors.red.shade50,
              child: Padding(
                padding: const EdgeInsets.all(16.0),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const Text(
                          'Total Debits',
                          style: TextStyle(fontSize: 12),
                        ),
                        Text(
                          '\$${totalDebit.toDecimalString()}',
                          style: const TextStyle(
                            fontSize: 18,
                            fontWeight: FontWeight.bold,
                          ),
                        ),
                      ],
                    ),
                    Column(
                      crossAxisAlignment: CrossAxisAlignment.end,
                      children: [
                        const Text(
                          'Total Credits',
                          style: TextStyle(fontSize: 12),
                        ),
                        Text(
                          '\$${totalCredit.toDecimalString()}',
                          style: const TextStyle(
                            fontSize: 18,
                            fontWeight: FontWeight.bold,
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 8),
            Center(
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 16,
                  vertical: 8,
                ),
                decoration: BoxDecoration(
                  color: totalDebit == totalCredit ? Colors.green : Colors.red,
                  borderRadius: BorderRadius.circular(20),
                ),
                child: Text(
                  totalDebit == totalCredit
                      ? 'Entry is Balanced'
                      : 'Entry is NOT Balanced',
                  style: const TextStyle(
                    color: Colors.white,
                    fontWeight: FontWeight.bold,
                  ),
                ),
              ),
            ),
            if (widget.entry.status == 'posted') ...[
              const SizedBox(height: 24),
              if (_correctionId != null)
                Text(
                  'This entry has been corrected. The correction is in the entries list.',
                  style: Theme.of(context).textTheme.bodySmall,
                )
              else
                FilledButton.tonalIcon(
                  onPressed: _reversing || !_canReverse ? null : _reverse,
                  icon: _reversing
                      ? const SizedBox(
                          height: 16,
                          width: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.undo),
                  label: const Text('Correct this entry'),
                ),
            ],
          ],
        ),
      ),
    );
  }
}
