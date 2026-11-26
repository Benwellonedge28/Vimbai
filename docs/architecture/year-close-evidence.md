# Work package 26 — Bind a year close to its monthly evidence

Source baseline `42a2d2f81c94b14394f86169429be9cef1339b89`. Proposed design.
## Scope
This package builds on [period reopening](period-reopening.md). `rust/crates/domains/accounting/src/year_end_close.rs::list_closed_years` currently infers reopened months from the current active-lock set. Once a month is explicitly reclosed, that inference loses the reopening as evidence. This package specifies how a new year close records what it actually relied on and how a reader verifies it later.

## Write contract
In `close_year`, capture the ordered twelve active monthly close record IDs read by the operation and save them with the year-close receipt and journal ID. If duplicate or missing active locks exist, refuse before generating the year-end entry. Check the same IDs again at commit under the period transition guard. A race in which any month is reopened or replaced returns “A month changed while the year was being closed. Review the month's close history and try again.” Persist the evidence and year-end posting atomically. Continue dating the entry according to the existing year-end logic and using the same ledger posting authority, balances and Book scope. Do not recognise a new year close merely because a report was read.

## Read contract and rollout
`list_closed_years` checks each recorded ID against its Book's corresponding month. A captured row reopened later marks that month as changed, even when a new row for that month is active. Add `evidence_status` with values `verified`, `superseded` and `unverified_legacy`. Preserve `months_reopened_since` as a compatibility projection. A legacy receipt lacking IDs may use provable historical reopening events but must remain unverified, even when all months are currently locked. Preserve all old closing journals and receipt records. An unresolved evidence read is an error requiring retry. Reclosing the whole year after an adjustment is a separate accounting decision outside this package.

Keep the existing routes in `rust/apps/api-server/src/accounting_book.rs`. Adjust `apps/flutter/lib/features/year_end/data/year_end_posting.dart` and `apps/flutter/lib/features/year_end/presentation/close_the_year_page.dart` to display the evidence status and name affected months. Use “The closing entry is recorded, but its monthly close evidence is unavailable” for legacy receipts. Use “Monthly close evidence changed after the year was closed” for superseded receipts.

## Verification
- Add `year_close_records_the_twelve_active_month_ids` to the year-end tests, including a neighbouring Book with the same periods.
- Add `reopened_then_reclosed_month_still_supersedes_year_evidence` to the year-end tests using two distinct close IDs.
- Add `year_close_racing_month_reopening_commits_no_stale_evidence` to provider conformance for SQLite and PostgreSQL. Assert atomicity of the journal and receipt.
- Add `legacy_year_close_displays_unverified_evidence` to Flutter's year-close tests.

Accept when a human can distinguish a verified close, a superseded close and a legacy close across restarts without any journal rewriting.
