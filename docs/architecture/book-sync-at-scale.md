# Sync a large Book with bounded reads and durable receipts

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Requires the access contract in 03 and supplies the replay boundary for 05, 09 and 20.

## Existing protocol

`rust/crates/platform/offline-sync/src/sync.rs::push_entries` checks write membership and the device owner, reads existing entries, plans conflicts and then writes entries, versions and Book cursors. Its `pull_entries` loads matching Book rows, filters and sorts by sequence, then truncates to the page size. `rust/crates/platform/offline-sync/src/conflict.rs::plan` defines conflict decisions. `apps/flutter/lib/features/books/data/book_sync_service.dart` holds the device caller. `rust/apps/sync-server/tests/sync_scenarios.rs` covers the running server.

## Store and wire changes

Add a Book-scoped indexed query to the database adapter for entries after a cursor and through a selected high-water sequence. Order by sequence and use stable entry IDs for integrity checks. Read only the requested page plus one row in Postgres and SQLite. Return `next_cursor`, `has_more` and the snapshot high-water mark. Retain the existing `book_seq` field for compatible clients. The Flutter caller persists the highest sequence actually committed to local storage, then asks for later entries. Never advance to the high-water mark simply because it appears in the response. When the snapshot expires, resume from the last applied sequence with the next authorised snapshot.

Add a semantic transaction to allocate Book sequence numbers and commit operation entries, record versions, a receipt and audit event together. Use a unique Book-and-entry identity and a payload digest. A recovered retry of matching bytes returns the original effect; different bytes under the same key return a conflict. Define limits for operations and bytes per batch and give clients a smaller retryable batch size on refusal. Apply conflict resolution before writes and advance finite cursors monotonically. This protocol replicates entries and metadata; all journal posting continues through its current authority.

## Recovery, migration and errors

Add indexes and check duplicates before enforcing uniqueness. Flag inconsistent sequences rather than silently renumbering stored history. Existing clients use legacy response fields during rollout. Display a retryable pause separately from lost membership or a conflicting edit. A stopped sync leaves the unsent queue visible and recoverable.

## Acceptance tests

- `pull_drains_a_large_book_without_missing_entries_while_new_ones_arrive` uses multiple pages against both providers.
- `concurrent_push_and_crash_keep_one_receipt_and_no_partial_batch` verifies allocation and rollback.
- `the_client_advances_only_to_the_last_durably_applied_entry` covers interruption during page application.
- Done when the same workload reports rows scanned, database calls and latency on named hardware and Book sizes, and the data set converges under retries with no lost or duplicate posting.
