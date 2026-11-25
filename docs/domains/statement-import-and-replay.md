# Import a bank statement with a reviewable receipt

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Requires Book authorization from 03 and statement matching from 04.

## Existing behavior

`rust/crates/domains/banking/src/bank_feed.rs` has single and batch import, `parse_mt940`, and `import_mt940`. The MT940 path suppresses every write error inside its loop. Its fallback identity can also collapse distinct rows with identical text. The direct import path differentiates duplicates but performs a read ahead of its write. The existing router surfaces are in `rust/apps/api-server/src/service_bank_feed.rs` and `rust/apps/api-server/src/records.rs`. The Flutter statement screen and its data contract are in `apps/flutter/lib/features/bank_reconciliation/`.

## Import, identity and replay

Use the existing bank feed domain as the import owner. First implement a strict MT940 adapter and add other formats as separate versions. Introduce a validation-only preview that accepts a capped upload, names the source account, currency and statement cutoff, and returns source row positions and clear parse errors. Represent debit/credit direction explicitly and normalize amounts with exact decimals. Retain the file hash and parser version in an import receipt. Avoid logging statement payloads. Keep malformed input outside the database.

On confirmation, validate the Book, account, preview hash and permissions again. Scope the batch idempotency key to Book, connection, content digest and parser revision. A reliable external row ID must be checked against the original payload fingerprint. For an ID-less statement, use the statement digest and source row position to preserve repeated identical lines within the same statement. Report potential duplicates across overlapping statements for review. Use a uniqueness constraint and transactional write in both stores, including the batch receipt. Either commit all valid new rows plus duplicate outcomes, or abort with no new rows. A previously committed replay returns the original receipt. A reused key carrying different data yields a conflict. Include each line’s imported, previously imported or needs-review state.

An imported feed row is evidence. The reconciliation feature reads approved imported rows and matches them to posted cash-book activity. Any later recording of a missing payment uses the owner-approved normal posting path. Imports by themselves leave ledger totals unchanged.

## Errors and compatibility

For a bad row, show its statement line number and the field to fix. A wrong currency or account stops confirmation before rows are written. Preserve existing feed IDs and prior reconciliation history. Map historical imports to a provenance-unknown state unless a durable receipt can identify their source. Keep existing legacy routes during a published compatibility window, and move the phone to the reviewed preview and confirmation flow.

## Acceptance tests

- `a_second_copy_of_the_same_statement_returns_the_verified_receipt` includes simultaneous uploads.
- `identical_idless_rows_on_one_statement_are_preserved` checks stable row identities and cross-statement ambiguity.
- `write_failure_rolls_back_the_batch_and_exposes_the_reason` injects a failure at the second write on both providers.
- `import_and_match_never_post_the_same_payment_twice` checks ledger isolation and subsequent accounting.
- Done when the existing HTTP suite and new fixtures cover cutoff, duplicate, exact amount and rollback behavior and the mobile screen reconciles an imported statement without retyping.
