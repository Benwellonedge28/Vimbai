import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/services/accounting_api_service.dart';
import 'package:finacc_mobile_client/models/accounting_models.dart';

class TrialBalancePage extends StatefulWidget {
  const TrialBalancePage({super.key});

  @override
  State<TrialBalancePage> createState() => _TrialBalancePageState();
}

class _TrialBalancePageState extends State<TrialBalancePage> {
  late Future<TrialBalance> _trialBalanceFuture;
  final AccountingApiService _apiService = AccountingApiService();

  @override
  void initState() {
    super.initState();
    _trialBalanceFuture = _apiService.getTrialBalance();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Trial Balance'),
          ),
          body: FutureBuilder<TrialBalance>(
            future: _trialBalanceFuture,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              } else if (snapshot.hasError) {
                return Center(child: Text('Error: ${snapshot.error}'));
              } else if (!snapshot.hasData) {
                return const Center(child: Text('No data for Trial Balance.'));
              } else {
                final trialBalance = snapshot.data!;
                return Padding(
                  padding: const EdgeInsets.all(8.0),
                  child: Column(
                    children: [
                      Text('Report Date: ${trialBalance.reportDate.toLocal().toString().split(' ')[0]}',
                          style: const TextStyle(fontWeight: FontWeight.bold)),
                      DataTable(
                        columnSpacing: 10,
                        horizontalMargin: 10,
                        columns: const [
                          DataColumn(label: Text('Account', style: TextStyle(fontWeight: FontWeight.bold))),
                          DataColumn(label: Text('Debit', style: TextStyle(fontWeight: FontWeight.bold)), numeric: true),
                          DataColumn(label: Text('Credit', style: TextStyle(fontWeight: FontWeight.bold)), numeric: true),
                        ],
                        rows: trialBalance.entries.map((entry) => DataRow(cells: [
                          DataCell(Text('${entry.accountNumber} ${entry.accountName}')),
                          DataCell(Text(entry.debitTotal.toStringAsFixed(2))),
                          DataCell(Text(entry.creditTotal.toStringAsFixed(2))),
                        ])).toList(),
                      ),
                      const Divider(),
                      Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 10.0, vertical: 5.0),
                        child: Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            const Text('TOTALS', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                            Text(trialBalance.totalDebits.toStringAsFixed(2), style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                            Text(trialBalance.totalCredits.toStringAsFixed(2), style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                          ],
                        ),
                      ),
                      if (trialBalance.totalDebits == trialBalance.totalCredits)
                        const Text('Trial Balance is Balanced!', style: TextStyle(color: Colors.green, fontWeight: FontWeight.bold))
                      else
                        const Text('Trial Balance is NOT Balanced!', style: TextStyle(color: Colors.red, fontWeight: FontWeight.bold)),
                    ],
                  ),
                );
              }
            },
          ),
        );
      }
    }
