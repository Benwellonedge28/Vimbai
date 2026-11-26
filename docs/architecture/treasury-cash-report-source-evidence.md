# Work package 76 — Freeze cash report balances and their conversion sources

Treasury-reporting review [3a2c2ca1](https://github.com/Benwellonedge28/Vimbai/commit/3a2c2ca16978df88a95ad6b1c1decb2148a8707b).

## The current report

`cash_position` in `rust/crates/domains/treasury/src/treasury_reporting.rs` adds the caller's `balance_usd` values independently of the native balance and currency. Missing amounts can default to zero, and the account count comes from the input length. A repeated account or contradictory conversion can therefore appear in a saved report. The stored entries provide no requirement to reference the observations that established those values.

## Build an evidence-backed report

Accept a report date, explicitly selected reporting currency, and unique scoped treasury or bank account IDs. Require a verified position revision for each account effective on or before the report date. Use [the observation contract](bank-connection-observation-evidence.md) or reviewed treasury evidence. Record the applicable amount type consistently for the run, such as current balance. Keep unknown observations separate. Never add a placeholder or mix current and available amounts in one subtotal.

Resolve a dated rate for each differing currency using [package 70](currency-conversion-rate-revisions.md). Save the chosen native observation ID, effective date, native amount, rate revision IDs, exact normalized amount and precision. Group native balances by currency. Publish a complete consolidated total only if every requested account has eligible evidence and each needed rate is available. An incomplete report publishes per-account reasons and retains the available native totals with coverage counts. Round each converted line under the reporting currency's policy and sum those displayed lines so the published total can be checked without hidden rounding. Keep a stable account order.

Save one immutable `TREASURY_REPORT` row referencing the frozen inputs and authenticated preparer. Replay of a scoped report key and identical inputs returns that report. A revised observation or rate creates a new report and leaves the earlier one accessible by ID. This reporting event records neither transfer instructions nor ledger postings.

## API and historical reports

Update `generate_cash_position_report` and the adapter calling `treasury_reporting::get_report` in `rust/apps/api-server/src/records.rs`. Add an opt-in evidence schema while preserving the legacy body and listing behavior. Keep existing report IDs and totals with historical status. No migrated row acquires verified labels from caller-supplied conversions alone.

For a repeated account say “Select each account once.” For unavailable inputs report “No verified balance or rate was available for this account on the report date” beside the scoped account. For a conflicting report key say “This report reference already identifies a different report.”

## Verification

- Add `cash_report_reconciles_rounded_lines_and_native_currency_totals` beside treasury-reporting tests, including two accounts sharing a currency.
- In `rust/apps/api-server/tests/records_http.rs`, add `cash_report_duplicate_account_cannot_double_total` and `saved_cash_report_keeps_rate_revisions_after_source_change`. Include an incomplete report and a foreign Book account.

Done when the report independently reconciles to the named observations and the same saved report can be read after balances and rates are updated.
