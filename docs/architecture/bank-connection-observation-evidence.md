# Work package 69 — Report bank synchronization and balances from observed evidence

Source review [3a2c2ca1](https://github.com/Benwellonedge28/Vimbai/commit/3a2c2ca16978df88a95ad6b1c1decb2148a8707b).

## Baseline

`start_sync` in `rust/crates/domains/banking/src/bank_feed.rs` creates a syncing record but fetches no provider data. `account_balance` labels returned figures as a placeholder. In `rust/crates/domains/banking/src/integration.rs`, `sync` stamps completion after checking connection status, and `balance_of` reports placeholder provenance. Both modules share the connection record. Feeding these results into cash decisions would confuse an attempted refresh with a completed import.

## Separate attempts from observations

Give both sync endpoints one scoped operation. Save an attempt with Book, connection ID, requested window and operation key. With no active observation source, finish the attempt as `source_unavailable` without changing the last verified sync time. Accepted sources for this work package are a confirmed import receipt from [statement import](../domains/statement-import-and-replay.md) or an authorized balance observation backed by a bank statement reference. A completed import records the covered window, source receipt ID and exact counts. A balance observation records currency, as-of date, current balance, optionally available balance, and its source. Never calculate an available balance by subtracting an assumed hold. Blank evidence cannot complete an attempt.

Reserve the operation key and persist the terminal attempt with its connection metadata in a scoped transaction. Only a verified import advances last successful sync. A later-recorded observation with an older effective date remains readable but cannot replace the newest effective balance. The versioned balance read returns each unobserved amount as null and states which date and source support available amounts. Require matching connection currency and prevent changes through a disconnected connection. The read does not sweep an account or send instructions to a bank.

## Contracts and transition

Update the source operations above and `service_bank_feed.rs` plus the records handlers `post_bank_sync`, `post_bank_sync_start` and `get_bank_sync_history`. Keep old query and route names and expose verified observations through an opt-in response. In `apps/flutter/lib/features/banking/data/banking_api_service.dart`, display “No verified bank balance is available” or “No source is connected for this refresh” for the corresponding missing state. When the connection is closed, show “This connection is disconnected. Its earlier observations are still available.” Preserve old sync attempts and timestamp fields with legacy provenance. Placeholder amounts never migrate into verified balances.

## Checks

- In `rust/apps/api-server/tests/banking_http.rs`, add `placeholder_refresh_cannot_advance_verified_sync`.
- In `bank_feed_http.rs`, add `balance_observations_order_by_effective_date_and_keep_unknown_available`, including two connections in the same Book.
- Add `bank_sync_attempt_and_completion_evidence_survive_replay` to the provider coverage proposed in [package 47](subsidiary-journal-posting-receipts.md). Exercise a missing evidence reference, duplicate key and failure at the metadata write.

The result is ready when both banking routes agree on the verified balance source and a failed refresh leaves the previous evidence readable.
