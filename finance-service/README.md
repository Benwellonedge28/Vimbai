# ... (existing content) ...

### **Financial Analysis Endpoints**

**(Requires `finance.read.variance_reports` permissions)**

#### Get Budget Variance Report

**Endpoint:** `GET http://localhost:8081/budgets/{budget_id}/variance-report` (via API Gateway)
Example: `GET http://localhost:8081/budgets/a1b2c3d4-e5f6-7890-1234-567890abcdef/variance-report`

**(Requires `finance.read.financial_ratios` permissions)**

#### Get Financial Ratios Report

**Endpoint:** `GET http://localhost:8081/financial-ratios` (via API Gateway)
**Query Parameters:**
- `start_date`: e.g., `2026-01-01T00:00:00Z`
- `end_date`: e.g., `2026-03-31T23:59:59Z`

Example: `GET http://localhost:8081/financial-ratios?start_date=2026-01-01T00:00:00Z&end_date=2026-03-31T23:59:59Z`

This endpoint now calculates a comprehensive suite of financial ratios, categorized as follows:

**Liquidity Ratios:**
-   **Current Ratio:** Current Assets / Current Liabilities
-   **Quick Ratio (Acid-Test Ratio):** (Cash + Marketable Securities + Accounts Receivable) / Current Liabilities
-   **Cash Ratio:** (Cash + Marketable Securities) / Current Liabilities

**Solvency (Leverage) Ratios:**
-   **Debt-to-Equity Ratio:** Total Liabilities / Total Equity
-   **Debt-to-Asset Ratio:** Total Liabilities / Total Assets
-   **Equity Multiplier:** Total Assets / Total Equity
-   **Interest Coverage Ratio:** EBIT / Interest Expense (EBIT is approximated as Net Income + Interest Expense + Tax Expense)

**Profitability Ratios:**
-   **Gross Profit Margin:** (Revenue - Cost of Goods Sold) / Revenue
-   **Operating Profit Margin:** Operating Income / Revenue (Requires explicit Operating Income data)
-   **Net Profit Margin:** Net Income / Revenue
-   **Return on Assets (ROA):** Net Income / Average Total Assets (Average Total Assets approximated as current Total Assets)
-   **Return on Equity (ROE):** Net Income / Average Total Equity (Average Total Equity approximated as current Total Equity)
-   **Earnings Per Share (EPS):** Net Income / Shares Outstanding (Requires external shares outstanding data)
-   **Return on Capital Employed (ROCE):** EBIT / Capital Employed (Capital Employed approximated as Total Assets - Current Liabilities)

**Efficiency (Activity) Ratios:**
-   **Inventory Turnover:** Cost of Goods Sold / Average Inventory (Requires COGS and Average Inventory, approximated as current Inventory)
-   **Accounts Receivable Turnover:** Credit Sales / Average Accounts Receivable (Credit Sales approximated as Total Revenue, Average AR approximated as current AR)
-   **Accounts Payable Turnover:** Cost of Goods Sold / Average Accounts Payable (Requires COGS and Average AP, approximated as current AP)
-   **Asset Turnover:** Total Revenue / Average Total Assets (Average Total Assets approximated as current Total Assets)
-   **Days Sales Outstanding (DSO):** 365 / Accounts Receivable Turnover
-   **Days Inventory Outstanding (DIO):** 365 / Inventory Turnover

**Market Ratios:**
-   **Price-Earnings Ratio (P/E):** Share Price / EPS (Requires external share price data)
-   **Dividend Yield:** Annual Dividend Per Share / Share Price (Requires external share price and dividend data)
*Note: Market ratios are currently placeholders as they require external market data not within FinAcc's current scope.*

---

### 5. Development

# ... (existing content) ...

## Error Handling

The service now employs custom exceptions and global exception handlers to provide more structured and informative error responses. Errors are returned in a consistent JSON format:
```json
{
  "detail": "Descriptive error message",
  "code": "ERROR_CODE_ENUM",
  "status_code": 404
}
```
Common error codes include: `NOT_FOUND`, `CONFLICT_ERROR`, `VALIDATION_ERROR`, `UNAUTHORIZED`, `FORBIDDEN`, `INVALID_DATE_RANGE`, `UPSTREAM_IS_FETCH_FAILED`, `UPSTREAM_IS_NETWORK_ERROR`, `UPSTREAM_BS_FETCH_FAILED`, `UPSTREAM_BS_NETWORK_ERROR`.

## Future Enhancements

-   More sophisticated Financial Analysis (e.g., trend analysis, forecasting, scenario modeling).
-   Integration with external data sources for market ratios (e.g., stock prices, shares outstanding).
-   Configurable ratio calculations based on user preferences.
