import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/budgeting/data/finance_api_service.dart';
import 'package:vimbai_mobile_client/features/budgeting/models/finance_models.dart';
import 'package:intl/intl.dart';

class FinancialRatiosPage extends StatefulWidget {
  const FinancialRatiosPage({super.key, this.loadRatios});

  /// Where the report comes from. The real service is the default; a test can hand in its own so the page is proven
  /// on the figures it is given rather than on a network.
  final Future<FinancialRatiosReport> Function(DateTime start, DateTime end)?
  loadRatios;

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
      final load = widget.loadRatios ?? _apiService.getFinancialRatios;
      _ratiosReportFuture = load(_startDate, _endDate);
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
        actions: [
          IconButton(
            icon: const Icon(Icons.date_range),
            tooltip: 'Select reporting period',
            onPressed: () => _selectDateRange(context),
          ),
        ],
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
                  Text(
                    'Report Date: ${DateFormat.yMMMd().format(report.reportDate)}',
                    style: const TextStyle(
                      fontSize: 16,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  Text(
                    'Period: ${DateFormat.yMMMd().format(report.startDate)} - ${DateFormat.yMMMd().format(report.endDate)}',
                  ),
                  const SizedBox(height: 20),
                  _buildRatioSection('Liquidity Ratios', [
                    _buildRatioRow(
                      'Current Ratio',
                      report.liquidity.currentRatio,
                    ),
                    _buildRatioRow('Quick Ratio', report.liquidity.quickRatio),
                    _buildRatioRow('Cash Ratio', report.liquidity.cashRatio),
                    _buildMoneyRow(
                      'Working Capital',
                      report.liquidity.workingCapital,
                      report.currency,
                    ),
                  ]),
                  const SizedBox(height: 20),
                  _buildRatioSection('Solvency Ratios', [
                    _buildRatioRow(
                      'Debt-to-Equity Ratio',
                      report.solvency.debtToEquityRatio,
                    ),
                    _buildRatioRow(
                      'Debt-to-Asset Ratio',
                      report.solvency.debtToAssetRatio,
                    ),
                    _buildRatioRow(
                      'Equity Multiplier',
                      report.solvency.equityMultiplier,
                    ),
                    _buildRatioRow(
                      'Times Interest Earned',
                      report.solvency.timesInterestEarned,
                    ),
                  ]),
                  const SizedBox(height: 20),
                  _buildRatioSection('Profitability Ratios', [
                    _buildRatioRow(
                      'Gross Profit Margin',
                      report.profitability.grossProfitMargin,
                      percentage: true,
                    ),
                    _buildRatioRow(
                      'Operating Profit Margin',
                      report.profitability.operatingProfitMargin,
                      percentage: true,
                    ),
                    _buildRatioRow(
                      'Net Profit Margin',
                      report.profitability.netProfitMargin,
                      percentage: true,
                    ),
                    _buildRatioRow(
                      'Return on Assets',
                      report.profitability.returnOnAssets,
                      percentage: true,
                    ),
                    _buildRatioRow(
                      'Return on Equity',
                      report.profitability.returnOnEquity,
                      percentage: true,
                    ),
                  ]),
                  const SizedBox(height: 20),
                  _buildRatioSection('Efficiency Ratios', [
                    _buildRatioRow(
                      'Inventory Turnover',
                      report.efficiency.inventoryTurnover,
                    ),
                    _buildRatioRow(
                      'Accounts Receivable Turnover',
                      report.efficiency.accountsReceivableTurnover,
                    ),
                    _buildRatioRow(
                      'Accounts Payable Turnover',
                      report.efficiency.accountsPayableTurnover,
                    ),
                    _buildRatioRow(
                      'Asset Turnover',
                      report.efficiency.assetTurnover,
                    ),
                    _buildRatioRow(
                      'Day Sales Outstanding',
                      report.efficiency.daySalesOutstanding,
                    ),
                  ]),
                  const SizedBox(height: 20),
                  // Only the ratios the engine actually answers for this Book are named here. Per-share figures need a
                  // share count the engine is never sent, so a row for them could only ever read "N/A".
                  _buildRatioSection('Market Value Ratios', [
                    _buildRatioRow(
                      'Price-to-Earnings Ratio',
                      report.marketValue.priceToEarningsRatio,
                    ),
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
        Text(
          title,
          style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
        ),
        const Divider(),
        ...ratios,
      ],
    );
  }

  /// A ratio is a comparison and reads as a decimal; money is not a ratio, so it keeps every digit it arrived with.
  Widget _buildMoneyRow(String label, ExactAmount? amount, String currency) =>
      _ratioRow(
        label,
        amount == null ? 'N/A' : '$currency ${amount.toString()}',
      );

  /// One line per reading. A figure with thirty digits is wider than a phone, so the value wraps under the label
  /// rather than running off the edge where it could not be read.
  Widget _ratioRow(String label, String displayValue) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4.0),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(child: Text(label, style: const TextStyle(fontSize: 16))),
          const SizedBox(width: 12),
          Flexible(
            child: Text(
              displayValue,
              textAlign: TextAlign.end,
              style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildRatioRow(
    String label,
    double? value, {
    bool percentage = false,
  }) {
    final displayValue = value == null
        ? 'N/A'
        : (percentage
              ? '${(value * 100).toStringAsFixed(2)}%'
              : value.toStringAsFixed(2));
    return _ratioRow(label, displayValue);
  }
}
