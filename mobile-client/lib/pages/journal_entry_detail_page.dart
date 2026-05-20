import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/models/accounting_models.dart';

class JournalEntryDetailPage extends StatelessWidget {
  final JournalEntry entry;
  const JournalEntryDetailPage({super.key, required this.entry});

  @override
  Widget build(BuildContext context) {
    double totalDebit = entry.lines.fold(0.0, (sum, line) => sum + line.debit);
    double totalCredit = entry.lines.fold(0.0, (sum, line) => sum + line.credit);

    return Scaffold(
          appBar: AppBar(
            title: const Text('Journal Entry Details'),
          ),
          body: SingleChildScrollView(
            padding: const EdgeInsets.all(16.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Description: ${entry.description}', style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                Text('Entry Date: ${entry.entryDate.toLocal().toString().split(' ')[0]}'),
                Text('Reference: ${entry.referenceNumber ?? 'N/A'}'),
                Text('Source: ${entry.sourceModule}'),
                Text('Entry ID: ${entry.id ?? 'N/A'}'),
                const SizedBox(height: 20),
                const Text('Lines:', style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
                const SizedBox(height: 10),
                DataTable(
                  columnSpacing: 10,
                  horizontalMargin: 10,
                  columns: const [
                    DataColumn(label: Text('Account', style: TextStyle(fontWeight: FontWeight.bold))),
                    DataColumn(label: Text('Description', style: TextStyle(fontWeight: FontWeight.bold))),
                    DataColumn(label: Text('Debit', style: TextStyle(fontWeight: FontWeight.bold)), numeric: true),
                    DataColumn(label: Text('Credit', style: TextStyle(fontWeight: FontWeight.bold)), numeric: true),
                  ],
                  rows: entry.lines.map((line) => DataRow(cells: [
                    DataCell(Text(line.accountNumber)),
                    DataCell(Text(line.description ?? '')),
                    DataCell(Text(line.debit.toStringAsFixed(2))),
                    DataCell(Text(line.credit.toStringAsFixed(2))),
                  ])).toList(),
                ),
                const Divider(),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 10.0, vertical: 5.0),
                  child: Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      const Text('TOTALS', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                      Text(totalDebit.toStringAsFixed(2), style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                      Text(totalCredit.toStringAsFixed(2), style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                    ],
                  ),
                ),
                const SizedBox(height: 10),
                Text(
                  totalDebit == totalCredit ? 'Entry is Balanced' : 'Entry is NOT Balanced',
                  style: TextStyle(color: totalDebit == totalCredit ? Colors.green : Colors.red, fontWeight: FontWeight.bold),
                ),
              ],
            ),
          ),
        );를
      }
    }
