# Move a deployment store and prove the destination is complete

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Depends on ADR-007, ADR-011, 15 and 16. This design covers an operator-managed store migration. Package 05 covers end-user device recovery.

## Existing tool and limits

`rust/apps/store-move/src/lib.rs` has `cutover` for planning and `move_guarded` for execution. The latter checks provider capabilities and refuses an occupied target by default. `move_all` takes a dump, restores it and compares a destination dump taken with the same per-kind limit. `rust/crates/adapters/database/src/dump.rs` carries records and edges, and `Dump::take` uses that limit for each kind. Two bounded dumps can agree while other source data remains outside the selected rows. Check existing coverage in `rust/apps/store-move/tests/cutover.rs` and `move.rs`.

## Cutover protocol

Add a versioned command receipt recording source and destination aliases, schema versions, source snapshot cursor, per-kind counts and digests, edge totals, evidence manifests and verification results. Use redacted connection aliases rather than credential-bearing DSNs in receipts and logs. Preflight must verify supported target capabilities, compatible schema and empty staging destination. Discover total counts from the snapshot independently of page size. Export pages in stable key order and refuse completion when a limit truncates a kind. Verify payload digests against the manifest before accepting a stage.

Before capturing the final snapshot, quiesce writes from the gateway, workers and sync server. Use a consistent source read for records and relationships, and record the highest accepted operation sequence. Restore into isolated staging without new client writes. Compare IDs, contents, edges, sequence, posting hash chains and per-Book trial balances. Verify actual attachment availability before cutover where the Book holds attachments. Point clients at the verified destination only after the operator approves the receipt. Keep the source read-only through the rollback window. A move interrupted mid-stage resumes only with the same snapshot and destination identity; otherwise rebuild staging from the source.

## Failures and rollback

Reject target writes on an occupied or incompatible destination. Report every discrepancy by kind or Book and preserve the source. A failed comparison blocks route switching. Rolling back after the destination has accepted writes requires reconciling the destination’s new operations before reopening the source, so acknowledged writes survive. Preserve schema and tool compatibility for retained exports.

## Acceptance tests

- `cutover_refuses_a_dump_capped_below_the_actual_record_count` covers matching truncated snapshots.
- `resume_after_interruption_keeps_sequence_and_evidence_complete` runs against an isolated supported destination.
- `a_digest_or_trial_balance_mismatch_blocks_switching` verifies the operator safeguard.
- Done when a rehearsal moves a representative store, proves the destination’s balances and attachment hashes match, and produces a reviewed rollback receipt.
