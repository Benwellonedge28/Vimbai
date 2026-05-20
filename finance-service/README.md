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

---

### 5. Development

# ... (existing content) ...

## Future Enhancements

-   More sophisticated Financial Analysis (e.g., trend analysis, detailed ratio breakdown).
-   Forecasting and Scenario Modeling endpoints.
-   Capital Budgeting tools.
