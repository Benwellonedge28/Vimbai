# Work package 74 — Reconcile every installment in a debt amortization schedule

Debt-management source reviewed at [3a2c2ca1](https://github.com/Benwellonedge28/Vimbai/commit/3a2c2ca16978df88a95ad6b1c1decb2148a8707b).

## Existing calculation

`paydown_rows` in `rust/crates/domains/treasury/src/debt_management.rs` calculates an unrounded running balance, then rounds the payment, principal component, interest and balance separately. Those published fields can fail to reconcile. `summary` estimates interest as payment times term less original principal rather than adding displayed interest rows. The realtime `quick_amortisation` caller and the stored-loan schedule already share the paydown function.

## Specify rounding on a closed schedule

For the exact schedule contract, accept positive principal, a nonnegative nominal annual decimal rate, a positive monthly term within the configured schedule row budget and the reporting currency's supported scale. Reject an over-budget term before allocating schedule rows. Retain the existing zero-rate division and fixed monthly rate convention. Parse numbers directly into `Money`. Compute the theoretical installment with decimal compounding and round it using the selected currency policy. For each nonfinal row, interest is rounded opening balance times the monthly rate. Principal is the nominal installment less that interest, capped at opening balance. Payment is the sum of the two components and closing balance is opening less principal. Refuse terms whose rounded installment cannot reduce a positive balance at the selected precision. On the final row set principal to the remaining balance, add that row's interest, and close exactly at zero. If a cap clears the loan early, stop and report the shorter effective schedule. Record precision and rate convention in the result.

For example, a zero-rate loan of 100 over three months at two decimals gives principal payments 33.33, 33.33 and 33.34. Each row reconciles and cumulative principal is 100. Derive versioned portfolio interest and representative payments from the same schedule. Aggregate only compatible currencies. Keep original-term schedule distinct from remaining outstanding debt. No schedule records a disbursement or repayment.

## Published callers and old data

Update `debt_loan_schedule` and the realtime caller of `quick_amortisation` in `rust/apps/api-server/src/records.rs`. Keep both existing response shapes while allowing an explicit exact schedule version. The input unit difference remains clear: stored annual rates are fractions and the realtime payload expresses percent. Convert once at the boundary. Older loan rows keep original terms and balances. No stored installment rewrite is required because the schedule is calculated from the selected loan. A bad historical term remains visible as a validation failure.

Say “Enter a positive loan term within the supported range” for invalid duration and “These terms cannot reduce the balance at the selected precision” when the rounded installment cannot amortize it.

## Proof

- In the debt unit tests add `exact_debt_schedule_assigns_rounding_remainder_to_final_installment` and `each_debt_row_reconciles_at_nonzero_rate`.
- In `rust/apps/api-server/tests/records_http.rs` add `stored_loan_and_realtime_preview_use_the_same_exact_schedule`, covering explicit rate unit conversion and another Book's loan ID.

Done when every published row balances and the sum of scheduled principal equals the original principal at the declared precision.
