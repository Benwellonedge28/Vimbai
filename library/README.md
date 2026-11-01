//! Vimbai calculation libraries (safe Rust only).
//!
//! Every crate in this workspace is pure, stateless computation:
//! numbers in, results out. No I/O, no database, no sockets, no
//! shared mutable state. Every crate sets `#![forbid(unsafe_code)]`.
//!
//! # Layout (one subdirectory per domain library)
//!
//! | Crate | Domain | Source services to port |
//! |------|--------|--------------------------|
//! | `variance` | Variance analysis | labour-cost, labour-efficiency, labour-rate, material-cost, material-price, material-usage, sales-price, sales-volume, variance (9) |
//! | `capital_valuation` | Capital budgeting & valuation math | net-present-value, internal-rate-return, payback-period, discounted-payback-period, discount-factor, accounting-rate-return, profitability-index (in npv svc), cost-of-capital, business-valuation, merger-valuation, eva, croic, mva, du-pont, asset-allocation, asset-turnover, risk-return, sustainable-growth, initial-investment, investment-appraisal, capital-budgeting, deal-structuring, divestiture, post-merger, synergy, business-valuation (~25) |
//! | `cost_accounting` | Cost & management accounting | absorption-costing, actual-cost, activity-based-costing, budgeted-cost, cost-accounting, equivalent-units, job-costing, lifecycle-costing, limiting-factor, make-or-buy, marginal-costing, order-acceptance, over-under-absorption, overhead-absorption-rate, prime-cost, process-costing, product-costing, standard-cost, target-costing, throughput-accounting, total-production-cost, cvp-analysis, margin-safety (~18) |
//! | `budgeting` | Budget preparation | budget, master-budget, cash-budget, capital-expenditure-budget, scenario-budget, zero-based-budget, flexible-budget, cash-flow, cash-budget (~9) |
//! | `ifrs` | IFRS / standards | ifrs-15, ifrs-16, ifrs-2, ifrs-9, revenue-recognition, government-grants, construction-contracts, events-after-reporting, interim-financial-reporting, segment-reporting, related-party, going-concern, esg-reporting, xbrl-reporting (~14) |
//! | `treasury_fx` | Treasury, FX & derivatives | foreign-exchange, foreign-currency-translation, options-pricing, forward-contracts, futures-hedging, commodity-hedging, currency-hedging, interest-rate-hedging, cross-currency-swap, credit-derivatives, exotic-derivatives, interest-rate-risk, market-risk, credit-risk-analysis, liquidity-forecast, liquidity-risk, financial-distress-prediction, cash-flow-forecasting (~22) |
//! | `tax` | Tax math | tax-calculation, tax-service, group-tax, r-and-d-tax, tax-provision, tax-return-preparation, tax-risk (7) |
//! | `assets` | Assets & depreciation | depreciation, fixed-assets, investment-property, biological-assets, intangible-assets, disposal-account, fixed-assets-schedule, amortization, goodwill, revaluation-model, inventory-valuation, net-realizable-value, impairment (13) |
//! | `ratios_statements` | Ratios & statement generation | consolidation, comparative-financial-statements, income-statement, financial-statements, ratio-analysis, management-accounts, management-reporting, annual-report, financial-reporting, ifrs-reporting, working-capital, working-capital-optimization, working-capital-finance, profit-loss-account, trading-account, balance-sheet, suspense-error, control-account-reconciliation, basis-apportionment, bank-fee-analysis, times-interest-earned, transfer-pricing, provisions (23) |
//! | `equity_payroll` | Partnership, equity, payroll, pension | partnership-accounting, partnership-sale, eps, share-options, payroll, payroll-accounting, pension-accounting, bonus-shares, right-issues, share-premium, share-redemption, capital-redemption-reserve, authorized/issued/ordinary/preference-shares, retained-profits, corporate-governance, director-emoluments (17+) |
//!
//! # Rounding policy
//!
//! The original Python services return floats rounded with Python's
//! `round()` (round-half-to-even on the binary double). To guarantee
//! bit-identical outputs when these libraries replace the HTTP calls,
//! use [`vimbai_common::py_round2`] etc. rather than ad-hoc rounding.
EOF

## Python seam (`native/`)

`library/native` builds `vimbai_native`, a PyO3 extension that exposes the
calculation crates to the Python services. It is its own workspace so the
calculation crates never link against libpython.

It is also the only crate in the project without `#![forbid(unsafe_code)]`:
PyO3 needs `unsafe` to cross the interpreter boundary. It holds no financial
logic, only adapters that parse decimal strings into `Money`, call a safe
library function, and render the result back as a string.

```bash
cd library/native
maturin build --release -o dist
pip install dist/*.whl
```

Services reach it through `shared/native_seam.py`, which raises rather than
falling back to float maths when the wheel is missing. `tests/test_native_seam.py`
runs the same request through the treasury analytics service and the Rust seam
and compares every KPI, so the two implementations cannot drift silently.

The treasury analytics service now computes its KPI set through this seam
(`treasury-analytics-service/engine.py`). `VIMBAI_CALC_ENGINE` picks the
calculator: `auto` uses Rust when the wheel is present, `rust` refuses to
start without it, `python` pins the reference implementation. `/health`
reports which one answered, so a deployment running on the fallback says so
rather than leaving you to guess.
