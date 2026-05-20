import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/services/accounting_api_service.dart';
import 'package:finacc_mobile_client/models/accounting_models.dart';
import 'package:intl/intl.dart';

class IncomeStatementPage extends StatefulWidget {
  const IncomeStatementPage({super.key});

  @override
  State<IncomeStatementPage> createState() => _IncomeStatementPageState();
}

class _IncomeStatementPageState extends State<IncomeStatementPage> {
  final AccountingApiService _apiService = AccountingApiService();
  late DateTime _startDate;
  late DateTime _endDate;
  Future<IncomeStatement>? _incomeStatementFuture;

  @override
  void initState() {
    super.initState();
    _endDate = DateTime.now(); // Current date
    _startDate = DateTime(_endDate.year, _endDate.month - 3); // Last 3 months
    _fetchIncomeStatement();
  }

  Future<void> _fetchIncomeStatement() async {
    setState(() {
      _incomeStatementFuture = _apiService.getIncomeStatement(_startDate, _endDate);
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
        _fetchIncomeStatement();
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Income Statement'),
            actions: [
              IconButton(
                icon: const Icon(Icons.date_range),
                onPressed: () => _selectDateRange(context),
              ),
            ],
          ),
          body: FutureBuilder<IncomeStatement>(
            future: _incomeStatementFuture,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              } else if (snapshot.hasError) {
                return Center(child: Text('Error: ${snapshot.error}'));
              } else if (!snapshot.hasData) {
                return const Center(child: Text('No data for Income Statement.'));
              } else {
                final statement = snapshot.data!;
                return SingleChildScrollView(
                  padding: const EdgeInsets.all(16.0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('Period: ${DateFormat.yMMMd().format(statement.startDate)} - ${DateFormat.yMMMd().format(statement.endDate)}',
                          style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                      const SizedBox(height: 10),
                      _buildSection('Revenues', statement.revenues, isRevenue: true),
                      const SizedBox(height: 10),
                      _buildSection('Expenses', statement.expenses, isRevenue: false),
                      const SizedBox(height: 20),
                      Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          const Text('Net Income', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                          Text('\$${statement.netIncome.toStringAsFixed(2)}', style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                        ],
                      ),
                    ],
                  ),
                );
              }
            },
          ),
        );
      }

      Widget _buildSection(String title, List<IncomeStatementItem> items, {required bool isRevenue}) {
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
