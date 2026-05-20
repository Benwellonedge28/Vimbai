// ... (Existing imports and AccountingApiService class definition) ...

      // --- Financial Statement API Calls (NEW ADDITIONS) ---
      Future<IncomeStatement> getIncomeStatement(DateTime startDate, DateTime endDate) async {
        final headers = await _getHeaders();
        final response = await http.get(
          Uri.parse('$_accountingServiceUrl/financial-statements/income-statement?start_date=${startDate.toIso8601String()}&end_date=${endDate.toIso8601String()}'),
          headers: headers,
        );

        if (response.statusCode == 200) {
          return IncomeStatement.fromJson(json.decode(response.body));
        } else {
          throw Exception('Failed to load income statement: ${response.body}');
        }
      }

      Future<BalanceSheet> getBalanceSheet(DateTime asOfDate) async {
        final headers = await _getHeaders();
        final response = await http.get(
          Uri.parse('$_accountingServiceUrl/financial-statements/balance-sheet?as_of_date=${asOfDate.toIso8601String()}'),
          headers: headers,
        );

        if (response.statusCode == 200) {
          return BalanceSheet.fromJson(json.decode(response.body));
        } else {
          throw Exception('Failed to load balance sheet: ${response.body}');
        }
      }

      Future<CashFlowStatement> getCashFlowStatement(DateTime startDate, DateTime endDate) async {
        final headers = await _getHeaders();
        final response = await http.get(
          Uri.parse('$_accountingServiceUrl/financial-statements/cash-flow-statement?start_date=${startDate.toIso8601String()}&end_date=${endDate.toIso8601String()}'),
          headers: headers,
        );

        if (response.statusCode == 200) {
          return CashFlowStatement.fromJson(json.decode(response.body));
        } else {
          throw Exception('Failed to load cash flow statement: ${response.body}');
        }
      }
    }
