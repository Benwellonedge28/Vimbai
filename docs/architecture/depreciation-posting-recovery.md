# Work package 29 — Resume depreciation after an interrupted posting run

Source baseline `e85043f61d13a2bef8f0d9fcdc066c559c09c3e6`. Proposed design.

## Baseline and boundary

`post_month` in `rust/crates/domains/accounting/src/depreciation.rs` iterates register rows and invokes `post_row`. `post_row` posts using the asset/period reference, then stamps the register row. `post_one` uses the same operation. An error can arrive after earlier rows have committed, or between one posting and its stamp. The goal is recoverability per asset and period.

## Per-row posting

Keep the existing Book-scoped asset and period reference. Read the register row at commit, verify it belongs to the requested Book and period, check the effective date, and validate matching accounts and exact amount. Commit journal and stamp atomically. An already stamped row returns its journal without new postings. If a concurrent call posted that row, resolve it to the same receipt. If a conflicting schedule row appears for the same key, report “More than one depreciation charge was found for this asset and month. Review its register.” Retain the existing ledger entry construction and closed-period refusal for fresh writes.

On a monthly run, report committed and already posted rows with journal IDs, plus an optional stopped-at row when an error interrupts processing. Leave remaining rows unposted. Return a structured domain error for the interruption with these details rather than reporting success for the batch. A retry resumes from the unposted set. Monthly totals for this attempt distinguish new charges from previously recorded charges. A scheduled month close that depends on completed depreciation must wait when processing is incomplete.

## Read and client changes

In `list_posted` and `unposted`, use verified linkage to distinguish posted, unposted and needs review. A legacy unstamped row can be repaired only after matching the existing Book journal's asset/period reference, module, date and lines. Preserve pre-existing rows and never repost an ambiguous legacy entry. The accounting handlers in `rust/apps/api-server/src/accounting_book.rs` pass through the structured result. Keep current success fields for `apps/flutter/lib/features/depreciation/data/depreciation_posting.dart`; update `post_depreciation_page.dart` in the corresponding presentation directory to explain whether any rows posted before an interruption.

## Tests

- `depreciation_row_stamp_failure_rolls_back_its_journal` injects storage faults on SQLite and PostgreSQL.
- `depreciation_month_retry_skips_committed_rows` forces an error on the second of three assets, then resumes.
- `single_and_monthly_depreciation_race_posts_once` checks the shared key by racing both commands.
- `unstamped_legacy_depreciation_requires_verified_lines` covers the repair and needs-review branches.

Accept when completed row IDs survive interruption and restart, and each posted asset-period changes the ledger once.
