# ... (existing content) ...

#### Get Financial Ratios Report

**Endpoint:** `GET http://localhost:8081/financial-ratios` (via API Gateway)
**Query Parameters:**
- `start_date`: e.g., `2026-01-01T00:00:00Z`
- `end_date`: e.g., `2026-03-31T23:59:59Z`

Example: `GET http://localhost:8081/financial-ratios?start_date=2026-01-01T00:00:00Z&end_date=2026-03-31T23:59:59Z`

**Response Structure (Expanded to include more ratios):**
```json
{
  "report_date": "2026-05-20T14:30:00Z",
  "start_date": "2026-01-01T00:00:00Z",
  "end_date": "2026-03-31T23:59:59Z",
  "liquidity": {
    "current_ratio": 2.50,
    "quick_ratio": 1.80,
    "cash_ratio": 0.50,
    "working_capital": 50000.00
  },
  "solvency": {
    "debt_to_equity_ratio": 0.80,
    "debt_to_asset_ratio": 0.45,
    "equity_multiplier": 1.80,
    "times_interest_earned": 5.00
  },
  "profitability": {
    "gross_profit_margin": 0.40,
    "operating_profit_margin": 0.25,
    "net_profit_margin": 0.15,
    "return_on_assets": 0.10,
    "return_on_equity": 0.18
  },
  "efficiency": {
    "inventory_turnover": 4.00,
    "accounts_receivable_turnover": 8.00,
    "accounts_payable_turnover": 6.00,
    "asset_turnover": 1.20,
    "day_sales_outstanding": 45.63
  },
  "market_value": {
    "earnings_per_share": null, 
    "price_to_earnings_ratio": null,
    "book_value_per_share": null
  },
  "currency": "USD"
}
```

---

### 5. Development

# ... (existing content) ...

## Future Enhancements

-   More sophisticated Financial Analysis (e.g., trend analysis, predictive modeling).
-   Forecasting and Scenario Modeling endpoints.
-   Capital Budgeting tools.
-   Detailed mapping of Chart of Accounts to financial statement line items for more precise ratio calculations.
