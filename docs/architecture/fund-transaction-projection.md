# Work package 46 — Keep fund transaction history and balances consistent

Proposed design against `b8826fac84d19d1792f7f95dc552f32d8f053755`.

## Observed behavior
`rust/crates/domains/accounting/src/fund_accounting.rs::record_transaction` creates a row, scans up to 1,000 Book transactions and filters by fund, then updates cached totals. A large Book can omit transactions before filtering, and a partial failure can leave the cache stale.

## Recording contract
Validate that the fund belongs to the Book and the new amount is positive, exact and in Book currency. Require valid date and an explicit income/expense classification for the new command. Accept a transaction ID to identify a retry and bind it to the canonical input. In a fund-scoped transaction, insert the row once and update the existing income, expense and net-asset projection from the complete fund history or a verified prior projection. Store enough checkpoint evidence to detect a stale cached projection. Use `totals` and `net_assets` as the single arithmetic definitions. This function records fund activity and does not create ledger entries.

If the projection cannot be verified or rebuilt, say “This fund’s totals could not be verified. Retry after its history has been checked.” Roll back the new row. A changed request with the same identity returns a conflict. Preserve `id` and `status` for the accounting fund-transaction adapter in `rust/apps/api-server/src/records.rs`. Keep the separate NPO adapter outside this work package until its own storage path is audited.

## Existing data and tests
Rebuild each existing fund from a complete, scoped transaction read. Keep original records. If an older row is unreadable, flag the affected fund instead of replacing an amount with zero. Record a rebuild receipt with the input count and before/after totals.

- `fund_projection_includes_rows_after_old_book_scan_limit` uses unrelated transactions ahead of the target fund's history.
- `fund_transaction_and_projection_rollback_together` faults the projection update on both stores.
- `fund_retry_returns_original_transaction_without_double_counting` races two writers.
- `fund_rebuild_reports_unreadable_legacy_amount` verifies no misleading balance is published.

Done when a complete replay matches the persisted fund totals on both stores and a malformed historical row remains diagnosable.
