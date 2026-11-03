# Service consolidation and Rust port plan

Generated from the live gateway config (`api-gateway/config/services.json`), 229 routed services,

## Ported and removed so far

Nine services are now safe Rust inside `library/capital_valuation`
(`time_value.rs`, `growth.rs`, `allocation.rs`, `appraisal.rs`). They are
de-registered: no gateway route, no k8s document, no bracket member. Their
Python sources stay on disk as the parity oracle for the crate, marked retired
in their `STATUS.md`. Gateway is at 220 routes.

`time-value-of-money-service`, `present-value-service`, `mva-service`,
`sustainable-growth-service`, `asset-allocation-service`,
`portfolio-optimization-service`, `capital-allocation-service`,
`investment-appraisal-service`, `project-appraisal-service`.

Everything below is the remaining plan.

## How the 229 split

* 96 do business arithmetic with no persistence anywhere in the directory: Rust-port and merge candidates.
* 101 keep accounting records in Neo4j or sqlite: merge candidates, grouped below.
* 32 are platform plumbing (bus, websocket, auth, ETL, files, policy): keep as services.

## Move to the safe-Rust workspace

These need no storage, so their maths belongs in `library/` crates behind the existing
PyO3 seam (`VIMBAI_CALC_ENGINE`). Each list is already a crate or a new module in one.

### `reporting` - 15 services, 2874 LOC

statements and dashboards.

* `analytical-procedures-service`
* `analytics-service`
* `bank-fee-analysis-service`
* `consolidation-service`
* `dashboard-service`
* `double-entry-principles-service`
* `kpi-tracking-service`
* `management-accounts-service`
* `management-dashboard-service`
* `profit-loss-account-service`
* `profit-service`
* `ratio-analysis-service`
* `suspense-error-service`
* `times-interest-earned-service`
* `trial-balance-service`

### `capital_valuation` - 13 services, 1885 LOC

NPV, IRR, payback, profitability index, present value, TVM, MVA, sustainable growth, investment and project appraisal, capital allocation.

* `asset-allocation-service`
* `capital-allocation-service`
* `internal-rate-return-service`
* `investment-appraisal-service`
* `mva-service`
* `net-present-value-service`
* `payback-period-service`
* `portfolio-optimization-service`
* `present-value-service`
* `profitability-index-service`
* `project-appraisal-service`
* `sustainable-growth-service`
* `time-value-of-money-service`

### `ifrs` - 11 services, 1501 LOC

IFRS presentation, ESG, going concern, governance, controls, XBRL.

* `audit-evidence-service`
* `audit-sampling-service`
* `corporate-governance-service`
* `esg-reporting-service`
* `events-after-reporting-service`
* `going-concern-service`
* `internal-control-evaluation-service`
* `internal-controls-testing-service`
* `statutory-filing-service`
* `substantive-testing-service`
* `xbrl-reporting-service`

### `treasury_fx` - 11 services, 1767 LOC

FX, pooling, interest-rate and credit risk, distress.

* `cash-pooling-service`
* `credit-risk-analysis-service`
* `financial-distress-prediction-service`
* `foreign-exchange-service`
* `fraud-risk-assessment-service`
* `interest-rate-risk-service`
* `market-risk-service`
* `operational-risk-service`
* `payment-optimization-service`
* `regulatory-ratio-service`
* `risk-return-analysis-service`

### `assets` - 9 services, 1799 LOC

inventory, biological, construction, lease, goodwill, provisions.

* `biological-assets-service`
* `construction-contracts-service`
* `disposal-account-service`
* `goodwill-service`
* `inventory-valuation-service`
* `investment-property-service`
* `lease-management-service`
* `provisions-contingencies-service`
* `revaluation-model-service`

### `corporate_events` - 8 services, 1272 LOC

valuation, merger, synergy, due diligence.

* `business-combination-service`
* `business-valuation-service`
* `deal-structuring-service`
* `divestiture-service`
* `due-diligence-service`
* `merger-valuation-service`
* `post-merger-service`
* `synergy-analysis-service`

### `tax` - 8 services, 1065 LOC

tax, VAT and transfer pricing.

* `group-tax-service`
* `r-and-d-tax-service`
* `tax-provision-service`
* `tax-return-preparation-service`
* `tax-risk-service`
* `tax-service`
* `transfer-pricing-service`
* `vat-reporting-service`

### `equity_payroll` - 7 services, 924 LOC

shares, options, emoluments, pension, related party.

* `director-emoluments-service`
* `options-pricing-service`
* `partnership-accounting-service`
* `partnership-sale-service`
* `pension-accounting-service`
* `related-party-service`
* `share-options-service`

### unmapped - 14 services

* `accounts-payable-service` (162 LOC)
* `accounts-receivable-service` (140 LOC)
* `acquisition-financing-service` (139 LOC)
* `balanced-scorecard-service` (130 LOC)
* `debt-covenants-service` (191 LOC)
* `financial-planning-service` (105 LOC)
* `fund-management-service` (113 LOC)
* `general-ledger-service` (102 LOC)
* `government-grants-service` (119 LOC)
* `household-finance-service` (141 LOC)
* `investment-portfolio-service` (121 LOC)
* `journal-entries-service` (110 LOC)
* `net-realizable-value-service` (141 LOC)
* `project-accounting-service` (122 LOC)

## Merge into domain services

Merge by domain, not by name. Identical route signatures are the cheapest mergers because
the contract is already shared.

### `corporate-equity-service` - 17 services, 15001 LOC

share, equity, dividend, debenture, reserve and appropriation records.

* `appropriation-control-service`
* `authorized-share-capital-service`
* `bonus-shares-service`
* `capital-reconstruction-service`
* `capital-redemption-reserve-service`
* `debentures-service`
* `equity-changes-service`
* `fund-accounting-service`
* `general-reserve-service`
* `issued-share-capital-service`
* `ordinary-shares-service`
* `preference-shares-service`
* `retained-profits-service`
* `revaluation-reserve-service`
* `right-issues-service`
* `share-premium-service`
* `share-redemption-service`

### `treasury-service` - 12 services, 9039 LOC

treasury, liquidity, hedging and trade finance.

* `cash-management-service`
* `cash-optimization-service`
* `financial-integrity-service`
* `financial-state-machine-service`
* `sovereign-treasury-service`
* `trade-finance-service`
* `treasury-analytics-service`
* `treasury-compliance-service`
* `treasury-management-service`
* `treasury-policy-service`
* `treasury-reporting-service`
* `treasury-risk-service`

### `audit-risk-service` - 11 services, 9123 LOC

audit engagements, compliance and risk registers.

* `audit-compliance-service`
* `forensic-accounting-service`
* `it-audit-service`
* `operational-audit-service`
* `org-authorization-engine`
* `policy-engine-service`
* `regulatory-compliance-service`
* `risk-assessment-service`
* `sox-compliance-service`
* `tax-audit-service`
* `zero-trust-data-service`

### `ledger-core-service` - 9 services, 10861 LOC

ledger, journals, reconciliation, suspense.

* `bank-feed-service`
* `bank-reconciliation-service`
* `cashbook-service`
* `control-account-reconciliation-service`
* `currency-service`
* `expense-tracking-service`
* `petty-cash-service`
* `purchases-ledger-control-service`
* `sales-ledger-control-service`

### `costing-service` - 8 services, 7330 LOC

costing and overhead.

* `absorption-costing-service`
* `absorption-costing-statement-service`
* `basis-apportionment-service`
* `departmental-accounting-service`
* `job-costing-service`
* `overhead-absorption-rate-service`
* `process-costing-service`
* `product-costing-service`

### `budgeting-service` - 6 services, 4995 LOC

budgets, forecasts, scenarios.

* `activity-based-budget-service`
* `rolling-forecast-service`
* `scenario-analysis-service`
* `scenario-modeling-service`
* `sensitivity-analysis-service`
* `zero-based-budgeting-service`

### `assets-service` - 6 services, 5001 LOC

asset registers and schedules.

* `disposal-group-service`
* `fixed-assets-register-service`
* `fixed-assets-schedule-service`
* `insurance-claims-service`
* `intangible-assets-service`
* `lease-termination-service`

### `statements-reporting-service` - 5 services, 5340 LOC

financial statements and reporting.

* `accounting-standards-service`
* `balance-sheet-service`
* `ifrs-reporting-service`
* `reporting-service`
* `trading-account-service`

### `receivables-payables-service` - 5 services, 4419 LOC

AP, AR, bad debt, intercompany.

* `bad-debts-recovery-service`
* `debt-management-service`
* `intercompany-service`
* `provision-bad-debts-service`
* `provision-doubtful-debts-service`

### `npo-personal-service` - 4 services, 8636 LOC

NPO and personal finance.

* `family-community-group-service`
* `npo-scale-service`
* `npo-service`
* `personal-finance-service`

### `partnership-service` - 4 services, 3028 LOC

partnership lifecycle.

* `partnership-agreement-service`
* `partnership-changes-service`
* `partnership-dissolution-service`
* `partnership-revaluation-service`

### `tax-service` - 3 services, 2937 LOC

tax, VAT.

* `tax-accounting-service`
* `tax-compliance-service`
* `tax-planning-service`

### `payroll-hr-service` - 1 services, 850 LOC

payroll and benefits.

* `benefits-admin-service`

### unclassified - 12 services

* `accounting-service` (7867 LOC)
* `bank-relationship-service` (817 LOC)
* `cash-flow-service` (641 LOC)
* `cash-flow-statement-service` (652 LOC)
* `company-accounting-service` (1502 LOC)
* `continue-shutdown-decision-service` (724 LOC)
* `cvp-analysis-service` (932 LOC)
* `initial-investment-service` (674 LOC)
* `investment-monitoring-service` (908 LOC)
* `revenue-recognition-service` (703 LOC)
* `selling-price-reduction-service` (721 LOC)

## Stubs that carry a ledger's name but hold no records

These parse as record-keeping services but have no Neo4j or sqlite access
anywhere in the directory - they only expose `/analyze` (or similar) and compute
a figure from the request body. Nothing has to be migrated before they fold into
the domain service above; they simply stop being separate containers.

* `general-ledger-service`, `journal-entries-service`
* `accounts-payable-service`, `accounts-receivable-service`
* `trial-balance-service`, `double-entry-principles-service`
* `bank-reconciliation-service`, `suspense-error-service`
* `cashbook-service`, `expense-tracking-service`

## Cheapest wins first

1. 19 services share exactly `[/, /analyze]` and none of them persist anything:
   one crate module and one route replaces nineteen containers.
2. The 4 audit services (`forensic-accounting`, `it-audit`, `operational-audit`,
   `tax-audit`) already share the same engagement route set and record shape,
   so they merge without touching their contract.
3. 7 services share `[/, /assess]` and are stateless: one `assessment` module.
4. The 10 stubs above join `ledger-core` with no data migration.
