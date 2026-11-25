# accounting-service

Status: proposed feature specification; catalogue evidence at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Actual handler implementation not yet audited.

## Existing published surface
Owner: `rust/crates/domains/accounting`.

- `DELETE /accounting/accounts/{}`
- `DELETE /accounting/journal-entries/{}`
- `GET /accounting/accounts`
- `GET /accounting/accounts/{}`
- `GET /accounting/accounts/{}/period-activity`
- `GET /accounting/accounts/{}/postings`
- `GET /accounting/accruals`
- `GET /accounting/bad-debts`
- `GET /accounting/balance-sheet`
- `GET /accounting/bank-reconciliation`
- `GET /accounting/bank-reconciliation/latest/{}`
- `GET /accounting/capital-calculations`
- `GET /accounting/capital-calculations/{}`
- `GET /accounting/cash-disbursements-journal`
- `GET /accounting/cash-flow`
- `GET /accounting/cash-receipts-journal`
- `GET /accounting/closed-periods`
- `GET /accounting/control-accounts`
- `GET /accounting/depreciation`
- `GET /accounting/depreciation/unposted`
- `GET /accounting/doubtful-debts`
- `GET /accounting/exchange-differences`
- `GET /accounting/expenses-journal`
- `GET /accounting/find`
- `GET /accounting/fx-balances`
- `GET /accounting/general-ledger`
- `GET /accounting/income-statement`
- `GET /accounting/journal-entries`
- `GET /accounting/journal-entries/{}`
- `GET /accounting/ledger/integrity`
- `GET /accounting/ledgers/{}`
- `GET /accounting/opening-balances`
- `GET /accounting/payroll`
- `GET /accounting/petty-cash-entries`
- `GET /accounting/petty-cash-funds`
- `GET /accounting/prepayments`
- `GET /accounting/profit-estimations`
- `GET /accounting/progress`
- `GET /accounting/purchases-journal`
- `GET /accounting/receipts-payments`
- `GET /accounting/sales-journal`
- `GET /accounting/sales-journal/{}`
- `GET /accounting/single-entry-conversions`
- `GET /accounting/statements-of-affairs`
- `GET /accounting/statements-of-affairs/{}`
- `GET /accounting/stock-adjustments`
- `GET /accounting/subsidiary-ledgers/accounts-payable`
- `GET /accounting/subsidiary-ledgers/accounts-receivable`
- `GET /accounting/subsidiary-ledgers/fixed-assets`
- `GET /accounting/subsidiary-ledgers/inventory`
- `GET /accounting/tax`
- `GET /accounting/tax-ledger`
- `GET /accounting/taxes`
- `GET /accounting/trial-balance`
- `GET /accounting/vendor-bills`
- `GET /accounting/year-end-close`
- `POST /accounting/accounts`
- `POST /accounting/accruals`
- `POST /accounting/accruals/reverse`
- `POST /accounting/bad-debts`
- `POST /accounting/bank-reconciliation`
- `POST /accounting/capital-calculations`
- `POST /accounting/cash-disbursements-journal`
- `POST /accounting/cash-receipts-journal`
- `POST /accounting/closed-periods`
- `POST /accounting/control-accounts`
- `POST /accounting/depreciation`
- `POST /accounting/depreciation/one`
- `POST /accounting/doubtful-debts`
- `POST /accounting/expenses-journal`
- `POST /accounting/fx-difference`
- `POST /accounting/fx-revaluation`
- `POST /accounting/fx-settlement`
- `POST /accounting/journal-entries`
- `POST /accounting/journal-entries/{}/post`
- `POST /accounting/journal-entries/{}/reverse`
- `POST /accounting/opening-balances`
- `POST /accounting/payroll`
- `POST /accounting/petty-cash-entries`
- `POST /accounting/petty-cash-funds`
- `POST /accounting/prepayments`
- `POST /accounting/prepayments/release`
- `POST /accounting/profit-estimations`
- `POST /accounting/purchases-journal`
- `POST /accounting/purchases-returns-journal`
- `POST /accounting/receipts-payments`
- `POST /accounting/sales-journal`
- `POST /accounting/sales-returns-journal`
- `POST /accounting/single-entry-conversions`
- `POST /accounting/statements-of-affairs`
- `POST /accounting/stock-adjustments`
- `POST /accounting/suspense/clear`
- `POST /accounting/tax-ledger/settle`
- `POST /accounting/taxes`
- `POST /accounting/taxes/compute`
- `POST /accounting/taxes/retire`
- `POST /accounting/taxes/seed`
- `POST /accounting/vendor-bills`
- `POST /accounting/year-end-close`
- `PUT /accounting/accounts/{}`
- `PUT /accounting/journal-entries/{}`

## Proposed design
Separate capture, draft validation, approval, posting, reversal and reporting. Account resolution follows ADR-015: provisional ordinary accounts may be created atomically only after implementation approval; control accounts must be configured. Preserve closed-period restrictions and exact amounts. Journal views, ledgers and trial balance derive from the same posted facts. Opening balances and single-entry conversion produce auditable balanced opening entries, not direct balance edits. Cash, sales, purchases and returns retain source document references. Accruals and prepayments carry release/reversal schedules. Tax and FX calculations delegate to canonical owners. Payroll, assets and vendor obligations require explicit cross-domain orchestration rather than duplicate ownership.

## Acceptance
Exact imbalance refusal, duplicate posting, reversal, closed period, control reconciliation, FX settlement, tax policy effective dates, opening balance conversion and report-to-ledger equality. Verify every listed route against handler and store before changes.

## Integration contract
Reuse existing domain handlers and versioned OpenAPI; API server composes cross-domain calls. Authenticate principal, enforce Book scope, use exact Money, idempotent writes and existing error envelope. Flutter exposes only authorized capabilities. Persist via existing adapter; ledger-core is sole posting integrity authority. Inventory concrete source files and tests before implementation; no new parallel service or store.
