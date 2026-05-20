import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/services/finance_api_service.dart';
import 'package:finacc_mobile_client/models/finance_models.dart';
import 'package:intl/intl.dart';

class FinancialRatiosPage extends StatefulWidget {
  const FinancialRatiosPage({super.key});

  @override
  State<FinancialRatiosPage> createState() => _FinancialRatiosPageState();
}

class _FinancialRatiosPageState extends State<FinancialRatiosPage> {
  final FinanceApiService _apiService = FinanceApiService();
  late DateTime _startDate;
  late DateTime _endDate;
  Future<FinancialRatiosReport>? _ratiosReportFuture;

  @override
  void initState() {
    super.initState();
    _endDate = DateTime.now(); // Current date
    _startDate = DateTime(_endDate.year, _endDate.month - 1); // Last month
    _fetchRatiosReport();
  }

  Future<void> _fetchRatiosReport() async {
    setState(() {
      _ratiosReportFuture = _apiService.getFinancialRatios(_startDate, _endDate);
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
        _fetchRatiosReport();
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Financial Ratios'),
          ),
          body: FutureBuilder<FinancialRatiosReport>(
            future: _ratiosReportFuture,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              } else if (snapshot.hasError) {
                return Center(child: Text('Error: ${snapshot.error}'));
              } else if (!snapshot.hasData) {
                return const Center(child: Text('No financial ratios data found.'));
              } else {
                final report = snapshot.data!;
                return SingleChildScrollView(
                  padding: const EdgeInsets.all(16.0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('Report Date: ${DateFormat.yMMMd().format(report.reportDate)}',
                          style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                      Text('Period: ${DateFormat.yMMMd().format(report.startDate)} - ${DateFormat.yMMMd().format(report.endDate)}'),
                      const SizedBox(height: 20),
                      _buildRatioSection('Liquidity Ratios', [
                        _buildRatioRow('Current Ratio', report.liquidity.currentRatio),
                        _buildRatioRow('Quick Ratio', report.liquidity.quickRatio),
                      ]),
                      const SizedBox(height: 20),
                      _buildRatioSection('Solvency Ratios', [
                        _buildRatioRow('Debt-to-Equity Ratio', report.solvency.debtToEquityRatio),
                        _buildRatioRow('Debt-to-Asset Ratio', report.solvency.debtToAssetRatio),
                      ]),
                      const SizedBox(height: 20),
                      _buildRatioSection('Profitability Ratios', [
                        _buildRatioRow('Gross Profit Margin', report.profitability.grossProfitMargin, percentage: true),
                        _buildRatioRow('Net Profit Margin', report.profitability.netProfitMargin, percentage: true),
                        _buildRatioRow('Return on Assets', report.profitability.returnOnAssets, percentage: true),
                      ]),
                    ],
                  ),
                );
              }
            },
          ),
        );
      }

      Widget _buildRatioSection(String title, List<Widget> ratios) {
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(title, style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
            const Divider(),
            ...ratios,
          ],
        );
      }

      Widget _buildRatioRow(String label, double? value, {bool percentage = false}) {
        String displayValue = value != null
            ? (percentage ? '${(value * 100).toStringAsFixed(2)}%' : value.toStringAsFixed(2))
            : 'N/A';
        return Padding(
          padding: const EdgeInsets.symmetric(vertical: 4.0),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(label, style: const TextStyle(fontSize: 16)),
              Text(displayValue, style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
            ],
          ),
        );
      }
    }
