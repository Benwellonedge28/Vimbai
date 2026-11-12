import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/accounting_api_service.dart';
import 'package:vimbai_mobile_client/models/accounting_models.dart';
import 'package:intl/intl.dart';

class BalanceSheetPage extends StatefulWidget {
  const BalanceSheetPage({super.key});

  @override
  State<BalanceSheetPage> createState() => _BalanceSheetPageState();
}

class _BalanceSheetPageState extends State<BalanceSheetPage> {
  final AccountingApiService _apiService = AccountingApiService();
  late DateTime _asOfDate;
  Future<BalanceSheet>? _balanceSheetFuture;

  @override
  void initState() {
    super.initState();
    _asOfDate = DateTime.now(); // Current date
    _fetchBalanceSheet();
  }

  Future<void> _fetchBalanceSheet() async {
    setState(() {
      _balanceSheetFuture = _apiService.getBalanceSheet(_asOfDate);
    });
  }

  Future<void> _selectDate(BuildContext context) async {
    final DateTime? pickedDate = await showDatePicker(
      context: context,
      initialDate: _asOfDate,
      firstDate: DateTime(2000),
      lastDate: DateTime(2101),
    );
    if (pickedDate != null) {
      setState(() {
        _asOfDate = pickedDate;
      });
      _fetchBalanceSheet();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Balance Sheet'),
            actions: [
              IconButton(
                icon: const Icon(Icons.date_range),
                onPressed: () => _selectDate(context),
              ),
            ],
          ),
          body: FutureBuilder<BalanceSheet>(
            future: _balanceSheetFuture,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              } else if (snapshot.hasError) {
                return Center(child: Text('Error: ${snapshot.error}'));
              } else if (!snapshot.hasData) {
                return const Center(child: Text('No data for Balance Sheet.'));
              } else {
                final statement = snapshot.data!;
                return SingleChildScrollView(
                  padding: const EdgeInsets.all(16.0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('As of Date: ${DateFormat.yMMMd().format(statement.asOfDate)}',
                          style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                      const SizedBox(height: 10),
                      _buildSection('Assets', statement.assets),
                      const SizedBox(height: 10),
                      _buildSection('Liabilities', statement.liabilities),
                      const SizedBox(height: 10),
                      _buildSection('Equity', statement.equity),
                      const SizedBox(height: 20),
                      Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          const Text('Total Assets', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                          Text('\$${statement.totalAssets.toStringAsFixed(2)}', style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                        ],
                      ),
                      const SizedBox(height: 10),
                      Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          const Text('Total Liabilities & Equity', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                          Text('\$${statement.totalLiabilitiesEquity.toStringAsFixed(2)}', style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                        ],
                      ),
                      const SizedBox(height: 10),
                      Text(
                        statement.totalAssets == statement.totalLiabilitiesEquity ? 'Balance Sheet is Balanced' : 'Balance Sheet is NOT Balanced',
                        style: TextStyle(color: statement.totalAssets == statement.totalLiabilitiesEquity ? Colors.green : Colors.red, fontWeight: FontWeight.bold),
                      ),
                    ],
                  ),
                );
              }
            },
          ),
        );
      }

      Widget _buildSection(String title, List<BalanceSheetItem> items) {
        double total = items.fold(0.0, (sum, item) => sum + item.amount);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(title, style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
            ...items.map((item) => Padding(
                  padding: const EdgeInsets.only(left: 16.0, top: 4),
                  child: Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      Text(item.category),
                      Text('\$${item.amount.toStringAsFixed(2)}'),
                    ],
                  ),
                )),
            const Divider(),
            Padding(
              padding: const EdgeInsets.only(left: 16.0),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text('Total $title', style: const TextStyle(fontWeight: FontWeight.bold)),
                  Text('\$${total.toStringAsFixed(2)}', style: const TextStyle(fontWeight: FontWeight.bold)),
                ],
              ),
            ),
          ],
        );
      }
    }
