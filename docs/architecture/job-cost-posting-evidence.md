# Work package 89 — Keep job cost receipts and running totals consistent

Job costing reviewed at [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Current separation

`add_cost` in `rust/crates/domains/costing/src/job_costing.rs` saves a `JOB_COST_ENTRY` then derives the new running cost from the previously read job row and writes it separately. Concurrent additions can overwrite one projection. `profitability` reads the cache and averages floating-point margins.

## One recorded cost, one revision

A versioned cost addition names job ID, expected revision, stable source operation key, effective date, verified Book currency and positive exact amount. Record the cost type and source reference. An attached ledger line must be visible in the same Book and satisfy the declared amount and date relationship. Distinguish a job allocation of an already posted cost from a planning entry. A planning entry must not be reported as posted income or expense. Correct a cost with a new linked adjustment, including reason and amount; never update the original entry silently.

Within a scoped transaction reserve the key, save the cost record and update the projection from the verified prior revision. Exact replay returns its original receipt. A mismatched replay or stale revision changes nothing. Store gross profit as contract value less recorded costs and compute portfolio margin as exact total profit divided by total contract value, null for zero denominator. If the legacy unweighted per-job average remains exposed, label it separately. Use the complete scoped company cohort for totals.

## Callers and older jobs

Update the `add_cost` and `profitability` adapters in `rust/apps/api-server/src/records.rs`. Keep legacy payload shapes, and prevent an old writer from bypassing a verified job's revision. Recompute legacy caches only after checking a complete cost history. Mark unexplained differences for review with both saved and replayed amounts. The treasury or accounting posting state is unaffected.

Say “This job changed. Reload it before submitting another cost” for a stale revision and “This cost reference already belongs to another record” for a changed replay.

## Acceptance evidence

- Add `job_profit_recomputes_exactly_from_its_cost_receipts` in the costing module and cover a zero contract.
- In the provider target proposed in [package 47](subsidiary-journal-posting-receipts.md), add `racing_job_costs_cannot_lose_a_delta_or_duplicate_a_receipt` and fault after entry insert.
- In `rust/apps/api-server/tests/costing_prefixes_http.rs`, add `job_portfolio_margin_uses_contract_weighted_exact_figures`.

Complete when each stored cost is traceable once in the job projection across both providers.
