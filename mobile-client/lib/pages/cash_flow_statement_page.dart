import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/services/accounting_api_service.dart';
import 'package:finacc_mobile_client/models/accounting_models.dart';
import 'package:intl/intl.dart';

class CashFlowStatementPage extends StatefulWidget {
  const CashFlowStatementPage({super.key});

  @override
  State<CashFlowStatementPage> createState() => _CashFlowStatementPageState();
}

class _CashFlowStatementPageState extends State<CashFlowStatementPage> {
  final AccountingApiService _apiService = AccountingApiService();
  late DateTime _startDate;
  late DateTime _endDate;
  Future<CashFlowStatement>? _cashFlowStatementFuture;

  @override
  void initState() {
    super.initState();
    _endDate = DateTime.now(); // Current date
    _startDate = DateTime(_endDate.year, _endDate.month - 3); // Last 3 months
    _fetchCashFlowStatement();
  }

  Future<void> _fetchCashFlowStatement() async {
    setState(() {
      _cashFlowStatementFuture = _apiService.getCashFlowStatement(_startDate, _endDate);
    });
  }

  Future<void> _selectDateRange(BuildContext context) async {
    final DateTime? pickedStartDate = await showDatePicker(
      context: context,
      initialDate: _startDate,
      firstDate: DateTime(2000),
      lastDate: DateTime(2101),
    );
    if (pickedStartDate != null) {
      final DateTime? pickedEndDate = await showDatePicker(
        context: context,
        initialDate: _endDate,
        firstDate: pickedStartDate,
        lastDate: DateTime(2101),
      );
      if (pickedEndDate != null) {
        setState(() {
          _startDate = pickedStartDate;
          _endDate = pickedEndDate;
        });
        _fetchCashFlowStatement();
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Cash Flow Statement'),
            actions: [
              IconButton(
                icon: const Icon(Icons.date_range),
                onPressed: () => _selectDateRange(context),
              ),
            ],
          ),
          body: FutureBuilder<CashFlowStatement>(
            future: _cashFlowStatementFuture,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              } else if (snapshot.hasError) {
                return Center(child: Text('Error: ${snapshot.error}'));
              } else if (!snapshot.hasData) {
                return const Center(child: Text('No data for Cash Flow Statement.'));
              } else {
                final statement = snapshot.data!;
                return SingleChildScrollView(
                  padding: const EdgeInsets.all(16.0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('Period: ${DateFormat.yMMMd().format(statement.startDate)} - ${DateFormat.yMMMd().format(statement.endDate)}',
                          style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                      Text('Report Date: ${DateFormat.yMMMd().format(statement.reportDate)}',
                          style: const TextStyle(fontSize: 14)),
                      const SizedBox(height: 10),
                      _buildSection(statement.operatingActivities),
                      _buildSection(statement.investingActivities),
                      _buildSection(statement.financingActivities),
                      const SizedBox(height: 20),
                      Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          const Text('Net Increase (Decrease) in Cash', style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                          Text('\$${statement.netIncreaseDecreaseInCash.toStringAsFixed(2)}', style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                        ],
                      ),
                      Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          const Text('Beginning Cash Balance', style: TextStyle(fontSize: 16)),
                          Text('\$${statement.beginningCashBalance.toStringAsFixed(2)}', style: const TextStyle(fontSize: 16)),
                        ],
                      ),
                      Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          const Text('Ending Cash Balance', style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                          Text('\$${statement.endingCashBalance.toStringAsFixed(2)}', style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                        ],
                      ),
                      const SizedBox(height: 10),
                      Text(
                        'Net Income: \$${statement.netIncome.toStringAsFixed(2)}',
                        style: const TextStyle(fontSize: 14, fontStyle: FontStyle.italic),
                      ),
                    ],
                  ),
                );
              }
            },
          ),
        );
      }

      Widget _buildSection(CashFlowSection section) {
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 8.0),
              child: Text(section.title, style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
            ),
            ...section.activities.map((activity) => Padding(
                  padding: const EdgeInsets.only(left: 16.0, top: 4),
                  child: Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      Text(activity.description),
                      Text('\$${activity.amount.toStringAsFixed(2)}'),
                    ],
                  ),
                )),
            const Divider(),
            Padding(
              padding: const EdgeInsets.only(left: 16.0),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text('Net Cash from ${section.title.split(' ')[2]}', style: const TextStyle(fontWeight: FontWeight.bold)),
                  Text('\$${section.netCash.toStringAsFixed(2)}', style: const TextStyle(fontWeight: FontWeight.bold)),
                ],
              ),
            ),
            const SizedBox(height: 10),
          ],
        );
      }
    }
