import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import 'package:finacc_mobile_client/models/accounting_models.dart';

class JournalEntryDetailPage extends StatelessWidget {
  final JournalEntryInDB entry;
  const JournalEntryDetailPage({super.key, required this.entry});

  @override
  Widget build(BuildContext context) {
    double totalDebit = entry.lines.fold(0.0, (sum, line) => sum + line.debit.toDouble());
    double totalCredit = entry.lines.fold(0.0, (sum, line) => sum + line.credit.toDouble());

    return Scaffold(
      appBar: AppBar(
        title: const Text('Journal Entry Details'),
      ),
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
                      style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
                    ),
                    const SizedBox(height: 12),
                    Row(
                      children: [
                        const Icon(Icons.calendar_today, size: 16),
                        const SizedBox(width: 8),
                        Text('Date: ${DateFormat.yMMMd().format(entry.entryDate)}'),
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
                JournalLineInDB line = entryMap.value;
                return Card(
                  margin: const EdgeInsets.symmetric(vertical: 4),
                  child: Padding(
                    padding: const EdgeInsets.all(12.0),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          'Line ${idx + 1}',
                          style: const TextStyle(fontWeight: FontWeight.bold, color: Colors.grey),
                        ),
                        const SizedBox(height: 8),
                        Row(
                          children: [
                            const Icon(Icons.account_balance, size: 16),
                            const SizedBox(width: 8),
                            Expanded(
                              child: Text(
                                line.accountNumber,
                                style: const TextStyle(fontWeight: FontWeight.bold),
                              ),
                            ),
                          ],
                        ),
                        const SizedBox(height: 4),
                        if (line.description != null && line.description!.isNotEmpty) ...[
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
                                const Text('Debit', style: TextStyle(fontSize: 12, color: Colors.grey)),
                                Text(
                                  '\$${line.debit.toDouble().toStringAsFixed(2)}',
                                  style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold, color: Colors.red),
                                ),
                              ],
                            ),
                            Column(
                              crossAxisAlignment: CrossAxisAlignment.end,
                              children: [
                                const Text('Credit', style: TextStyle(fontSize: 12, color: Colors.grey)),
                                Text(
                                  '\$${line.credit.toDouble().toStringAsFixed(2)}',
                                  style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold, color: Colors.green),
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
              color: totalDebit == totalCredit ? Colors.green.shade50 : Colors.red.shade50,
              child: Padding(
                padding: const EdgeInsets.all(16.0),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const Text('Total Debits', style: TextStyle(fontSize: 12)),
                        Text(
                          '\$${totalDebit.toStringAsFixed(2)}',
                          style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
                        ),
                      ],
                    ),
                    Column(
                      crossAxisAlignment: CrossAxisAlignment.end,
                      children: [
                        const Text('Total Credits', style: TextStyle(fontSize: 12)),
                        Text(
                          '\$${totalCredit.toStringAsFixed(2)}',
                          style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
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
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                decoration: BoxDecoration(
                  color: totalDebit == totalCredit ? Colors.green : Colors.red,
                  borderRadius: BorderRadius.circular(20),
                ),
                child: Text(
                  totalDebit == totalCredit ? 'Entry is Balanced' : 'Entry is NOT Balanced',
                  style: const TextStyle(color: Colors.white, fontWeight: FontWeight.bold),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}