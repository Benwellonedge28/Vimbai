# Calculator port tracker

Generated from the tree.

Rules for every port in this file: safe Rust only, money carried as `vimbai_common::Money`
(decimal, exact at any magnitude), amounts published as decimal strings from the library.
`library/HTTP.md` describes the surface that makes deletion possible; `scripts/retire_gate.py`
says which services have passed every check. 169 stateless calculators; 45 answer through the Rust library today, 17 of them served at /calc/.

Services that own records are ported into the Rust store and served from the API's record
routes; they are listed here too, because the Python service is no longer the engine behind them.

## Wired to the Rust library

- disposal-group-service -> assets crate, records on the store: groups, their assets, impairment
  tests and status changes. Adding an asset twice updates it instead of counting it twice.
- lease-termination-service -> assets crate, records on the store: terminations, settlements,
  approve and complete. The settlement is exact and the approval order is kept.
- fixed-assets-register-service -> assets crate, records on the store: register, list, get,
  depreciate, entries, dispose, summary.
- intangible-assets-service -> assets crate, records on the store: register, list, get, amortize,
  amortisation entries.
- revaluation-reserve-service -> assets crate, records on the store: revaluations, utilizations,
  the cumulative reserve. Journal entries are built and returned, not posted.
- fixed-assets-schedule-service -> assets crate, the same register plus its schedule doors:
  partial update, schedule generate and latest, the scheduled summary.
- biological-assets-service -> assets crate, six routes from `biological-assets-measure` to
  `biological-assets-disclosure-template`.
- disposal-account-service -> assets crate, `disposal-account-calculate` and `disposal-account-post`.
- investment-property-service -> assets crate, five routes from `investment-property-fair-value-model`
  to `investment-property-yields`.
- absorption-costing-service
- job-costing-service
- activity-based-costing-service
- actual-cost-service
- amortization-service
- appropriation-control-service
- asset-turnover-service
- balanced-scorecard-service
- budgeted-cost-service
- business-valuation-service
- capital-expenditure-budget-service
- cash-flow-statement-service
- cash-optimization-service
- consolidation-service
- corporate-tax-service
- croic-service
- depreciation-service
- divestiture-service
- du-pont-service
- eps-service
- equity-changes-service
- eva-service
- financial-distress-prediction-service
- fixed-assets-service
- fraud-risk-assessment-service
- goodwill-service
- government-grants-service
- group-tax-service
- internal-control-evaluation-service
- inventory-valuation-service
- net-realizable-value-service
- options-pricing-service
- partnership-accounting-service
- partnership-sale-service
- payment-optimization-service
- pension-accounting-service
- process-costing-service
- product-costing-service
- regulatory-ratio-service
- basis-apportionment-service
- cvp-analysis-service
- overhead-absorption-rate-service
- selling-price-reduction-service
- standard-cost-service
- substantive-testing-service
- target-costing-service
- tax-provision-service
- tax-risk-service
- throughput-accounting-service
- trading-account-service
- treasury-analytics-service
- vat-reporting-service
- insurance-claims-service
acquisition-financing-service
asset-allocation-service
audit-evidence-service
business-combination-service
business-documents-service
capital-allocation-service
commodity-hedging-service
corporate-governance-service
cost-accounting-service
currency-hedging-service
due-diligence-service
esg-reporting-service
financial-planning-service
foreign-currency-translation-service
futures-hedging-service
going-concern-service
investment-appraisal-service
lease-management-service
management-dashboard-service
mva-service
operational-risk-service
portfolio-optimization-service
project-accounting-service
provisions-contingencies-service
ratio-analysis-service
segment-reporting-service
statutory-filing-service
trial-balance-service
xbrl-reporting-service

## Crate already has the maths, service not yet wired

These are the cheapest ports left: the crate already holds the arithmetic, so the work is a
module that shapes the request and answer and a route in front of it. There are five, not the
forty-five this section used to list - the other forty named services whose directories were
deleted when their maths moved, and a list of services that no longer exist is not a backlog, it
is a list of things to go and check. `tests/test_port_tracker.py` now reads this file and fails if
an entry names a service that is not on disk.

- budget-service -> budgeting::prepare_budget, budgeting::monitor_budget
- payback-period-service -> capital_valuation::payback_period
- present-value-service -> capital_valuation::present_value, capital_valuation::present_value_percent
- reporting-service -> advisory::analyze_events_after_reporting
- rolling-forecast-service -> budgeting::rolling_forecast
## Needs the maths written in Rust

Services whose arithmetic has not been written in Rust. Each one still has a directory, which is
what keeps it in this list: the forty-five that used to sit here without one were services that had
already been deleted, and a to-do list of deleted code is not a plan.

- audit-sampling-service
- banking-integration-service
- biological-assets-service
- dashboard-service
- disposal-account-service
- distributed-workflow-orchestration-service
- double-entry-principles-service
- encrypted-backup-service
- etl-service
- foreign-exchange-service
- graphql-service
- identity-service
- internal-rate-return-service
- investment-property-service
- net-present-value-service
- payment-gateway-service
- performance-benchmarking-service
- privacy-admin-dashboard-service
- report-automation-service
- report-distribution-service
- revaluation-model-service
- websocket-service

### Deleted with the maths still unwritten

These were listed as needing their maths ported, and their services were deleted anyway. They are
not one backlog; they are two different situations, and telling them apart changes the work.

`scripts/module_reachability.py` answers which Rust modules any door can reach. A door is the API
server, the native extension, or the bridge they share - nothing else can call into the library. The
script follows imports through crates, dependency renames (`use treasury::{...}` for
`vimbai_service_treasury`) and re-exported functions (`assets::depreciation_batch_from_json`), because
each of those looks unwired to a naive search.

**Answered by maths that is reachable today.** The capability exists, some service calls it, and the
deleted service was one of several front doors to the same arithmetic:

- comparative-financial-statements-service is `ratios_statements::comparative`
- exotic-derivatives-service is `treasury_fx::exotics`
- financial-forecasting-service is `budgeting::forecasting`
- financial-reporting-service and financial-statements-service are `ratios_statements::statements`,
  routed as `financial-statements-prepare` (one body shape, two services that always shared it)
- merger-valuation-service is `capital_valuation::merger`, routed as `merger-valuation-value`
- profit-loss-account-service is `profit_loss` in the budgeting crate, routed as `profit-loss-prepare` and
  `profit-loss-compare`
- scenario-budget-service is `scenario_budget` in the budgeting crate, routed as `scenario-budget-analyze`
- suspense-error-service is `suspense` in the accounting crate, routed as `suspense-error-analyze`
- kpi-tracking-service is `reporting::analysis`, routed as `kpi-tracking-track`
- deal-structuring-service, post-merger-service and synergy-analysis-service share
  `capital_valuation::corporate_events`, routed as `deal-structuring-structure`,
  `post-merger-track` and `synergy-analysis-analyze`
- ifrs-9-financial-instruments-service is `ifrs::instruments`, routed as `ifrs-9-classify`,
  `-sppi-test`, `-business-model`, `-ecl-general`, `-ecl-simplified` and `-hedge-effectiveness`
- ifrs-2-share-based-payment-service is `ifrs::share_based`, routed as `ifrs-2-calculate`,
  `-fair-value-equity-instruments`, `-performance-condition`, `-modification` and
  `-group-transaction`
- ifrs-15-revenue-recognition-service is `ifrs::revenue`, routed as `ifrs-15-recognize`,
  `-variable-consideration`, `-standalone-selling-price`, `-progress-measurement`,
  `-customer-options` and `-modification`
- interim-financial-reporting-service is `ratios_statements::interim_reporting`
- labour-efficiency-variance-service is `cost_accounting::labour_variance`
- liquidity-management-service is `treasury_fx::liquidity`
- management-reporting-service is `reporting::management`
- regulatory-reporting-service is `reporting::regulatory`
- swap-valuation-service is `treasury_fx::derivatives`
- tax-calculation-service is `tax::corporate` behind the `tax-calculate` calculator
- time-value-of-money-service is `capital_valuation::capital_returns`
- treasury-cash-service is `service_treasury::cash_optimization`

**Arithmetic that exists and nothing can reach.** Empty. Every module the reachability script found with
no door has been wired: the last four were business valuation, merger valuation, and the three grouped
into their domain crates. `tests/test_unrouted_maths.py` still fails if a module loses its door while the
tracker says otherwise, and `tests/test_port_tracker.py` fails if a service is claimed wired without being
reachable.

## Retired, and what stands in for them

Samuel asked for the retired Python sources to be deleted, so a service can no longer be its own
evidence. What replaces it is either a recorded request/response capture in
`rust/apps/api-server/tests/parity`, or a calculator registration that answers the same question, or -
for the ones below with neither - a decision recorded here and pinned in `tests/test_port_tracker.py`,
because maths sitting in a crate that nothing can reach is not a port, it is an orphan.

Answered by a capture or a registered calculator:

- accounting-rate-return-service -> capital_valuation::accounting_rate_of_return
- activity-based-costing-service -> calculator
- actual-cost-service -> calculator
- amortization-service -> calculator
- asset-turnover-service -> calculator
- balanced-scorecard-service -> calculator
- budgeted-cost-service -> calculator
- capital-expenditure-budget-service -> calculator
- cash-budget-service -> budgeting::prepare_cash_budget
- consolidation-service -> calculator
- corporate-tax-service -> calculator
- cost-of-capital-service -> capital_valuation::cost_of_capital, capital_valuation::cost_of_capital_percent
- croic-service -> calculator
- cross-currency-swap-service -> treasury_fx::value_cross_currency_swaps
- depreciation-service -> calculator
- discount-factor-service -> capital_valuation::discount_factor, capital_valuation::discount_factor_percent
- divestiture-service -> calculator
- du-pont-service -> calculator
- eps-service -> calculator
- eva-service -> calculator
- financial-distress-prediction-service -> calculator
- fixed-assets-service -> calculator
- flexible-budget-service -> cost_accounting::flexible_budget
- fraud-risk-assessment-service -> calculator
- goodwill-service -> calculator
- government-grants-service -> calculator
- group-tax-service -> calculator
- interest-rate-hedging-service -> advisory::analyze_interest_rate_risk, treasury_fx::analyze_interest_rate_hedge
- internal-control-evaluation-service -> calculator
- inventory-valuation-service -> calculator
- management-accounts-service -> ratios_statements::prepare_management_accounts
- master-budget-service -> budgeting::prepare_master_budget
- multi-currency-service -> treasury_fx::validate_multi_currency_transaction
- net-realizable-value-service -> calculator
- options-pricing-service -> calculator
- partnership-accounting-service -> calculator
- partnership-sale-service -> calculator
- payment-optimization-service -> calculator
- pension-accounting-service -> calculator
- profit-service -> native::profit_json, advisory::analyze_profitability_index
- project-appraisal-service -> capital_valuation::project_appraisal
- regulatory-ratio-service -> calculator
- standard-cost-service -> calculator
- substantive-testing-service -> calculator
- sustainable-growth-service -> capital_valuation::sustainable_growth
- target-costing-service -> calculator
- tax-provision-service -> calculator
- tax-return-preparation-service -> tax::prepare_tax_return
- tax-risk-service -> calculator
- tax-service -> native::group_tax_json, native::tax_provision_json
- throughput-accounting-service -> calculator
- vat-reporting-service -> calculator
- zero-based-budget-service -> budgeting::prepare_zero_based_budget

Maths in a crate with nothing to reach it, kept as a recorded debt:

| service | where the maths is |
| --- | --- |
| annual-report-service | a calculator is not registered |
| bank-fee-analysis-service | ratios_statements::analyze_bank_fees, reporting::analyze_bank_fees |
| capital-budgeting-service | a calculator is not registered |
| cash-flow-forecasting-service | capital_valuation::present_value_of_cash_flows, performance_measures::from_cash_flow |
| credit-derivatives-service | treasury_fx::analyze_credit_derivatives |
| credit-risk-analysis-service | a calculator is not registered |
| discounted-payback-period-service | capital_valuation::discounted_payback |
| equivalent-units-service | cost_accounting::equivalent_units |
| forward-contracts-service | treasury_fx::analyze_forward_contracts |
| ifrs-16-lease-accounting-service | ifrs::calculate_ifrs16 |
| income-statement-service | cost_accounting::marginal_income_statement, ratios_statements::prepare_income_statement |
| internal-controls-testing-service | a calculator is not registered |
| labour-cost-variance-service | cost_accounting::actual_labour_cost, cost_accounting::labour_cost_rate |
| labour-rate-variance-service | cost_accounting::labour_hours_rate, cost_accounting::labour_cost_rate |
| liquidity-forecast-service | a calculator is not registered |
| liquidity-risk-service | a calculator is not registered |
| make-or-buy-decision-service | cost_accounting::analyze_make_or_buy |
| market-risk-service | a calculator is not registered |
| material-cost-variance-service | cost_accounting::actual_material_cost, cost_accounting::material_cost_variance |
| material-price-variance-service | variance::material_price_variance |
| material-usage-variance-service | variance::material_usage_variance |
| payroll-service | equity_payroll::payroll_run, equity_payroll::payroll_taxes |
| r-and-d-tax-service | a calculator is not registered |
| risk-return-analysis-service | capital_valuation::analyze_risk_return |
| sales-price-variance-service | variance::sales_price_volume_analysis |
| sales-volume-variance-service | variance::sales_price_volume_analysis, variance::sales_volume_cm_analysis |
| times-interest-earned-service | ratios_statements::analyze_times_interest_earned |
| treasury-operations-service | treasury_fx::analyze_treasury_operations |
| variance-service | budgeting::analyze_budget_variance, budgeting::detailed_variance_analysis |
| working-capital-finance-service | ratios_statements::analyze_working_capital, ratios_statements::optimize_working_capital |
| working-capital-optimization-service | ratios_statements::analyze_working_capital, ratios_statements::optimize_working_capital |
| working-capital-service | ratios_statements::analyze_working_capital, ratios_statements::optimize_working_capital |
## Divergences worth knowing

`depreciation-service` published a `/calculate` schedule that was synthetic: it was never wired
to the library, and its Python body did not call the seam. The Rust ladder in `vimbai_assets`
is the real implementation, so the figures from that one endpoint were never a like-for-like
reference. The `/method` endpoint was wired and watched by parity tests.

`selling-price-reduction-service` answered `break_even_volume_increase: Infinity` when the new
contribution per unit was not positive. That is not valid JSON, so a strict client saw a parse error
where the service meant "no volume breaks even". The port answers `null` for that case and leaves the
recommendation (`REJECT`/`CONDITIONAL_APPROVAL`) to carry the meaning; every other figure is the same
arithmetic, held in `Money` instead of floats.

`cvp-analysis-service` answered `Infinity` for the figures that do not exist when the contribution
is not positive (break-even volume and revenue, and - in a product mix - break-even and target
revenue). Those are `null` in the port. It also refused a non-positive contribution on
`/target-profit` with 400; the record ports answer 422 for every rejected submission, so that is
what a bad target-profit call gets now.

`overhead-absorption-rate-service` put a `formula` sentence in two of its answers built from Python's
float text ("8000.0 / 1000.0 = 8.0"). Those sentences are gone rather than reworded differently: the
fields beside them carry the same two figures the caller supplied, exactly, and a sentence built from
them would be a third copy to keep in step.

`basis-apportionment-service` multiplied the total by each proportion as a float and summed the
results, so the parts did not always add back to the cost being apportioned. The port splits with
largest-remainder allocation instead: the parts are the total, to six places, and `unapportioned` is
zero whenever there is a basis to spread over. A basis made entirely of zeros still leaves the whole
cost unapportioned, as before.
