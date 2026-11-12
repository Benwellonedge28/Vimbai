# Calculator port tracker

Generated from the tree.

Rules for every port in this file: safe Rust only, money carried as `vimbai_common::Money`
(decimal, exact at any magnitude), amounts published as decimal strings from the library.
`library/HTTP.md` describes the surface that makes deletion possible; `scripts/retire_gate.py`
says which services have passed every check. 169 stateless calculators; 45 answer through the Rust library today, 17 of them served at /calc/.

Services that own records are ported into the Rust store and served from the API's record
routes; they are listed here too, because the Python service is no longer the engine behind them.

## Wired to the Rust library

- absorption-costing-service
- job-costing-service
- activity-based-costing-service
- actual-cost-service
- amortization-service
- annual-report-service
- appropriation-control-service
- asset-turnover-service
- balanced-scorecard-service
- budgeted-cost-service
- capital-budgeting-service
- capital-expenditure-budget-service
- cash-flow-statement-service
- cash-optimization-service
- consolidation-service
- corporate-tax-service
- credit-risk-analysis-service
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
- internal-controls-testing-service
- inventory-valuation-service
- liquidity-forecast-service
- liquidity-risk-service
- market-risk-service
- net-realizable-value-service
- options-pricing-service
- partnership-accounting-service
- partnership-sale-service
- payment-optimization-service
- pension-accounting-service
- process-costing-service
- product-costing-service
- r-and-d-tax-service
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

## Crate already has the maths, service not yet wired

- accounting-rate-return-service -> capital_valuation::accounting_rate_of_return
- bank-fee-analysis-service -> ratios_statements::analyze_bank_fees, reporting::analyze_bank_fees
- budget-service -> budgeting::prepare_budget, budgeting::monitor_budget
- cash-budget-service -> budgeting::prepare_cash_budget
- cash-flow-forecasting-service -> capital_valuation::present_value_of_cash_flows, performance_measures::from_cash_flow
- cost-of-capital-service -> capital_valuation::cost_of_capital, capital_valuation::cost_of_capital_percent
- credit-derivatives-service -> treasury_fx::analyze_credit_derivatives
- cross-currency-swap-service -> treasury_fx::value_cross_currency_swaps
- discount-factor-service -> capital_valuation::discount_factor, capital_valuation::discount_factor_percent
- discounted-payback-period-service -> capital_valuation::discounted_payback
- equivalent-units-service -> cost_accounting::equivalent_units
- flexible-budget-service -> cost_accounting::flexible_budget
- forward-contracts-service -> treasury_fx::analyze_forward_contracts
- ifrs-16-lease-accounting-service -> ifrs::calculate_ifrs16
- income-statement-service -> cost_accounting::marginal_income_statement, ratios_statements::prepare_income_statement
- interest-rate-hedging-service -> advisory::analyze_interest_rate_risk, treasury_fx::analyze_interest_rate_hedge
- labour-cost-variance-service -> cost_accounting::actual_labour_cost, cost_accounting::labour_cost_rate
- labour-rate-variance-service -> cost_accounting::labour_hours_rate, cost_accounting::labour_cost_rate
- make-or-buy-decision-service -> cost_accounting::analyze_make_or_buy
- management-accounts-service -> ratios_statements::prepare_management_accounts
- master-budget-service -> budgeting::prepare_master_budget
- material-cost-variance-service -> cost_accounting::actual_material_cost, cost_accounting::material_cost_variance
- material-price-variance-service -> variance::material_price_variance
- material-usage-variance-service -> variance::material_usage_variance
- multi-currency-service -> treasury_fx::validate_multi_currency_transaction
- payback-period-service -> capital_valuation::payback_period
- payroll-service -> equity_payroll::payroll_run, equity_payroll::payroll_taxes
- present-value-service -> capital_valuation::present_value, capital_valuation::present_value_percent
- profit-service -> native::profit_json, advisory::analyze_profitability_index
- project-appraisal-service -> capital_valuation::project_appraisal
- reporting-service -> advisory::analyze_events_after_reporting
- risk-return-analysis-service -> capital_valuation::analyze_risk_return
- rolling-forecast-service -> budgeting::rolling_forecast
- sales-price-variance-service -> variance::sales_price_volume_analysis
- sales-volume-variance-service -> variance::sales_price_volume_analysis, variance::sales_volume_cm_analysis
- sustainable-growth-service -> capital_valuation::sustainable_growth
- tax-return-preparation-service -> tax::prepare_tax_return
- tax-service -> native::group_tax_json, native::tax_provision_json
- times-interest-earned-service -> ratios_statements::analyze_times_interest_earned
- treasury-operations-service -> treasury_fx::analyze_treasury_operations
- variance-service -> budgeting::analyze_budget_variance, budgeting::detailed_variance_analysis
- working-capital-finance-service -> ratios_statements::analyze_working_capital, ratios_statements::optimize_working_capital
- working-capital-optimization-service -> ratios_statements::analyze_working_capital, ratios_statements::optimize_working_capital
- working-capital-service -> ratios_statements::analyze_working_capital, ratios_statements::optimize_working_capital
- zero-based-budget-service -> budgeting::prepare_zero_based_budget

## Needs the maths written in Rust

- acquisition-financing-service
- asset-allocation-service
- audit-evidence-service
- audit-sampling-service
- banking-integration-service
- biological-assets-service
- business-combination-service
- business-documents-service
- business-valuation-service
- capital-allocation-service
- commodity-hedging-service
- comparative-financial-statements-service
- corporate-governance-service
- cost-accounting-service
- currency-hedging-service
- dashboard-service
- deal-structuring-service
- disposal-account-service
- distributed-workflow-orchestration-service
- double-entry-principles-service
- due-diligence-service
- encrypted-backup-service
- esg-reporting-service
- etl-service
- exotic-derivatives-service
- financial-forecasting-service
- financial-planning-service
- financial-reporting-service
- financial-statements-service
- foreign-currency-translation-service
- foreign-exchange-service
- futures-hedging-service
- going-concern-service
- graphql-service
- identity-service
- ifrs-15-revenue-recognition-service
- ifrs-2-share-based-payment-service
- ifrs-9-financial-instruments-service
- interim-financial-reporting-service
- internal-rate-return-service
- investment-appraisal-service
- investment-property-service
- kpi-tracking-service
- labour-efficiency-variance-service
- lease-management-service
- liquidity-management-service
- management-dashboard-service
- management-reporting-service
- merger-valuation-service
- mva-service
- net-present-value-service
- operational-risk-service
- payment-gateway-service
- payroll-accounting-service
- performance-benchmarking-service
- portfolio-optimization-service
- post-merger-service
- privacy-admin-dashboard-service
- profit-loss-account-service
- project-accounting-service
- provisions-contingencies-service
- ratio-analysis-service
- realtime-calculation-engine
- regulatory-reporting-service
- report-automation-service
- report-distribution-service
- revaluation-model-service
- scenario-budget-service
- segment-reporting-service
- statutory-filing-service
- suspense-error-service
- swap-valuation-service
- synergy-analysis-service
- tax-calculation-service
- time-value-of-money-service
- treasury-cash-service
- trial-balance-service
- websocket-service
- xbrl-reporting-service


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
