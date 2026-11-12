# Calculator port tracker

Generated from the tree.

Rules for every port in this file: safe Rust only, money carried as `vimbai_common::Money`
(decimal, exact at any magnitude), amounts published as decimal strings from the library.
`library/HTTP.md` describes the surface that makes deletion possible; `scripts/retire_gate.py`
says which services have passed every check. 169 stateless calculators; 45 answer through the Rust library today, 17 of them served at /calc/.

Services that own records are ported into the Rust store and served from the API's record
routes; they are listed here too, because the Python service is no longer the engine behind them.

## Wired to the Rust library

- accounting-service -> accounting crate, whole. Every route the service publishes is answered by the
  Rust engine, twenty of them as the core book: the chart of accounts (create, read by number, list,
  update, delete, period activity) and the journal (create with double-entry validation and a hash-chain
  stamp, read, list, delete, integrity), plus the ledger, trial balance, income statement and balance
  sheet computed from those entries. Amounts exact end to end, so a trial balance over
  123456789012345678901.55 keeps every digit.

  The rest are the books of original entry and the working papers beside the ledger. `subsidiary_journals`
  holds the six journals and the vendor bill, each recording its document and posting the double entry it
  implies in one act, so the record carries the id of the entry it generated; `bookkeeping` holds petty
  cash, bank reconciliation, statements of affairs, capital calculations, control accounts, receipts and
  payments, and the single-entry conversions.

  Shapes pinned rather than repaired: only `posted` entries move anything; an account is debit-normal only
  when its normal balance says `debit`; a trial balance moves a negative balance to the other column and
  calls itself balanced only on exact equality; the income statement reads period activity and drops an
  account whose movement runs against its natural side; the balance sheet lists every account of its three
  types and carries no profit for the period, so it can fail to balance; deleting an account is refused
  while a line touches it; a reference number is unique per source module only for the six modules the
  service checks. The journals post to the service's own fixed chart (`1100`/`4000` for a sale,
  `1200`/`2000` for a purchase, and so on), so a book whose chart lacks those numbers refuses the document
  instead of guessing where it belongs; tax is recorded on a document but never posted; a purchase return
  debits its expense account, raising it; a cash receipt books every source that is not a cash sale to
  `4100`; and both ageing ledgers owe the whole document with nothing paid against it, calling it overdue
  past thirty days whatever it was recorded as. The fixed-assets and inventory subsidiary ledgers answer
  with nothing at all, because they were placeholders and no figures are invented for them. A capital
  calculation given neither a profit nor a loss reports its opening capital as the closing one; a control
  account totals only the entry types it knows and reports each entry's balance one step behind itself; a
  receipts and payments account moves the bank balance only, leaving cash where it started; and petty cash
  vouchers never move the fund's imprest. A single-entry conversion and a profit estimation ask the same
  question and now call one calculation instead of two copies of it.

  Two deliberate changes: a posted entry cannot be edited or deleted, because the old update path silently
  broke its own stamp and every stamp after it, and corrections belong in reversing entries; and a
  reconciliation's per-line running balance is the running book balance, where the service reported the
  account's debit total less its credit total before each line, which is not a running balance at all. The
  shipped receivable and payable ledgers filtered on a date belonging to a node their query never matched,
  so the filter did nothing; the port filters on the document's own date.
- accounting-standards-service -> accounting crate. The catalogue is data, not records: fourteen
  standards and their requirements lifted from the service's own definitions into
  `crates/domains/accounting/assets/accounting-standards.json`, so nothing in it was retyped by hand.
  What a caller owns lives on the store and is Book-scoped: an organization's configuration, its account
  mappings, its accounting policies and its compliance history. Shapes pinned: configuring an
  organization replaces its configuration, and a policy is replaced per organization, standard and area;
  activating a standard already active (or deactivating one absent) writes nothing and answers as if it
  had; an unknown standard is refused but its requirements answer as an empty list; and a compliance
  check writes one check per requirement, passing unless the caller's validation results say otherwise,
  replacing the earlier history rather than appending to it.
- departmental-accounting-service -> accounting crate, records on the store: departments and their
  cost centres, allocation rules, cost pools, the allocation run, inter-department bills and their
  approval, the department financial summary and performance report, and the comparison and
  cost-distribution reports. Shapes kept as shipped and pinned: a pool's `included_departments` is never
  consulted (every active department it does not exclude is a target); the headcount basis looks at no
  figure at all and the expenses basis measures one department against a total made of that same
  department, so both land on an equal share; the revenue basis takes its figures from what the caller
  supplies; a second run of a pool replaces the first and closes it; the budget variance is budget less
  revenue, not less spending; and approving a bill returns its journal entry rather than posting it.
  Revenue, direct expenses and budget totals used to come from other services over HTTP and are now
  supplied by the caller, absent meaning zero.
- company-accounting-service -> accounting crate, records on the store: companies and the four equity
  accounts each one needs, the shareholder register, share capital, capital transactions, dividends and
  their per-shareholder payments, retained earnings, reserves, and the three reports (equity statement,
  shareholder register, dividend history). Shapes kept as shipped and pinned: registering a holder
  recalculates every holding in the company and leaves them alone when the total is zero; paid-up capital
  is issued shares times paid-up value, with premium carried separately; a dividend pays only holders of
  its own class at a flat ten percent withholding whatever the dividend itself declared; the equity
  statement takes the newest share capital and retained-earnings records. Journals are returned rather
  than posted, because posting belongs to whoever owns the ledger.
- the shareholder register has one writer: this port and issued-share-capital-service write the same
  `:ShareholderNode` rows through one shared shape in the store crate (accepting `name` or
  `shareholder_name`), and the equity route now registers through this port so a holding always has its
  percentage recalculated.
- cashbook-service -> accounting crate, records on the store: cash accounts (a code used once per
  Book), entries that move the account balance as they are written, posting and voiding, the account
  summary, bank reconciliations including the outstanding items, cash-flow entries with their summary,
  and the cash position with its history. Shapes kept as shipped and pinned: a new account opens at its
  opening balance and ignores any balance sent with it; an entry is always stored unreconciled; the
  summary's totals and closing balance count posted entries only while its transaction count covers
  every entry; posting and voiding do not check the entry's current state, so a voided entry can be
  posted and a second void reverses the balance again; the cash-flow summary's variance is expected
  minus actual while its net change is actual minus expected; and the position history runs from zero
  rather than from any opening balance.
- ifrs-reporting-service -> accounting crate, records on the store: reports limited to the five kinds,
  the filtered listing, the payload kept as given, disclosure notes, and approval. Adding a note also
  appends its number and title to the report's own disclosure index, so the report carries its notes;
  approval is from draft or reviewed only; and notes for a report that does not exist list empty while
  reading that report refuses. The IFRS calculators (revenue, share-based payment, instruments) stay in
  their own crates: this is the paperwork, not the arithmetic.
- intercompany-service -> accounting crate, records on the store: group entities, the transactions between
  them limited to the five known types, the filtered listing, matching a pair, and the eliminations.
  Four shapes kept as shipped and pinned: a match is exact equality with no tolerance and no currency
  check, so 100 USD matches 100 EUR; matching never checks the legs face each other, so two
  same-direction transactions match and eliminate with both sides reading from the same entity; and a
  refused transaction type or mismatched amount answers 422, the record routes' own status for refused
  input, rather than the original 400.
- cash-flow-service -> accounting crate, records on the store: a project's yearly inflows and outflows as
  entries, the totals, the kept schedules, and the three standalone flow calculators (totals, net flows
  with the shorter side padded, cumulative running total). Two shapes kept as shipped: the initial
  investment is recorded and never subtracted, so a project's net flow ignores what it cost to start;
  and reading a schedule that does not exist answers 200 with an error body rather than 404.
- balance-sheet-service -> accounting crate, records on the store: the sheet with its three totals computed
  server side, the history, the latest, and the ratios read off it. Balances to within a cent; every ratio
  denominator floored at 1, and no sheet at all is a 404 rather than an empty sheet. The float copy in
  the ratios crate was deleted with this: nothing reached it from any door, so it was a second set of
  balance-sheet arithmetic drifting from these figures.
- fund-accounting-service -> accounting crate, records on the store: funds with their derived net assets,
  the transactions recorded against them, and the company total. A transaction recomputes income and
  expenses from every transaction the fund has rather than adding itself on, and never moves the
  fund's balance - so a fund that receives a thousand shows more net assets and the same balance.
  Pinned as shipped rather than repaired, because a fund already on record would otherwise change.
- revenue-recognition-service -> accounting crate, records on the store: contracts with their obligations,
  the allocation of the transaction price by relative standalone selling price, recognition against one
  obligation at a time capped at what it is worth, and the company summary. Two shapes kept as shipped:
  an obligation with no standalone selling price keeps the price it was given, so the allocated parts
  need not add back to the total; and a recognition with no amount recognises the whole remaining
  obligation rather than nothing.
- capital-reconstruction-service -> equity crate, records on the store: the scheme with its reduction,
  its adjustments, its reserve conversions, and the approval entry that writes losses off against the
  reduction and balances to retained earnings. Only write-offs are posted; a transfer's new balance is
  stored but does not enter the entry, and approval is not guarded against running twice - both pinned
  rather than repaired.
- debentures-service -> equity crate, records on the store: classes (nominal, issue price, coupon,
  redemption price), issues booking the discount or premium, per-period interest with a flat twenty
  percent withheld, payment of it, and redemptions booking the premium. Accrual takes the outstanding
  count from the caller rather than the class, as the service did; an unrecognised payment frequency is
  treated as monthly.
- issued-share-capital-service -> equity crate, records on the store: issues, the shareholder register,
  and the capital summary. The issue entry credits share capital at one unit a share because the service
  hard-coded that nominal regardless of class, so preference and ordinary issues post the same capital
  and an issue below one unit posts negative premium; pinned by a test rather than repaired.
- initial-investment-service -> equity crate, records on the store for `/create` (the breakdown, total and
  net of scrap), with `/simple` and `/with-capital-allowance` as pure arithmetic that stores nothing.
  Zero lines are left out of the breakdown but still counted; scrap value never enters the simple outlay.
- share-redemption-service -> equity crate, records on the store: a redemption initiated, funded by a
  fresh issue, then completed, with the reserve requirement in between. Redeeming out of proceeds sets
  up a requirement for the nominal value and starts `awaiting_crr`; a fresh issue funds it and creates
  no reserve; every other method creates the nominal value. Exact throughout.
- share-premium-service -> equity crate, records on the store: premium entries computing (price less
  nominal) times shares, utilizations on four kinds, signed adjustments, and a balance of received less
  used plus adjusted. An issue below nominal records a negative premium rather than refusing.
- capital-redemption-reserve-service -> equity crate, records on the store: redemptions creating the
  reserve as proceeds less nominal (preference debits 3205, everything else 3200), manually created
  reserves from a capital reduction, utilizations, and a summary counting creations and redemptions as
  one figure.
- general-reserve-service -> equity crate, records on the store: reserves with their allocations and
  utilizations, the balance the single figure they share. A utilization past the balance is refused;
  a minimum or target balance is recorded and never enforced, as the service did.
- retained-profits-service -> equity crate, records on the store: appropriations with their
  carry-forward chain (total appropriated, carried forward, closing), interim dividends and prior-year
  adjustments; the summary adds carried forward to the adjustments for one company only.
- bonus-shares-service -> equity crate, records on the store: bonus issues and the reserve each
  capitalises, with the released amount kept as the total capitalised.
- right-issues-service -> equity crate, records on the store: rights offers with their acceptances and
  renunciations. An issue completes when every entitlement is covered, counting one unit per
  renunciation as the service did.
- ordinary-shares-service -> equity crate, records on the store: dividends declared and paid against
  the shares in issue, as one implementation shared with the preference path.
- preference-shares-service -> equity crate, records on the store: preference classes, their issuances
  and redemptions, dividends with arrears accumulated on the declared total. Journal entries are built
  and returned, not posted.
- authorized-share-capital-service -> equity crate, records on the store: share classes, their
  issuances and buybacks, and the used-capacity summary. The ceiling is enforced on issue and the
  issued count on buyback.
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
