# Work package 85 — Attach each bad-debt recovery to the write-off it reduces

Bad-debt recovery baseline [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Current recovery behavior

`record_recovery` in `rust/crates/domains/receivables/src/bad_debts.rs` rejects a zero amount but accepts a negative one. It adds the amount to the cached recovered balance before deciding completion, without checking the remaining recoverable amount. The function saves a recovery journal-shaped record before updating the bad-debt record. This is distinct from the [invoice-linked write-off](invoice-writeoff-evidence.md).

## Record externally observed recovery

Resolve the write-off record and its verified original amount under the current Book. Require a received date at or after the write-off, currency, positive exact amount and stable source receipt reference. Link already-posted accounting evidence only when the existing posting is visible and agrees. An unposted recovery receipt remains recorded evidence and makes no journal assertion. Recognize a given evidence reference once for this write-off. Refuse excess against the dated remaining amount and keep the excess available for separate accounting treatment.

Within the same scoped transaction save the recovery receipt and update the projection. Recompute or verify the prior projection from all receipt movements. Lock the debt revision to protect it from another recovery or correction. Return the original result on exact replay even after full recovery. Reversing a recorded recovery requires a new linked correction. Only the accounting authority can reverse an actual journal.

## Surface and old records

Change the recovery and summary adapters in `rust/apps/api-server/src/records.rs` to expose the receipt and effective balance for the versioned path while preserving legacy keys. Keep all old recovery records. Review missing source links, negative amounts and disagreement between cached and replayed amounts before migration. A verified checkpoint may retain an authorized legacy opening value and provenance. Repairs cannot create duplicate recovery evidence.

Show “Enter a positive amount received” for invalid amounts, “This recovery exceeds what remains written off” for excess and “Review earlier recovery records before continuing” for a failed reconciliation.

## Completion tests

- In the recovery module add `partial_recovery_cannot_exceed_the_remaining_writeoff`, including a negative amount.
- In `rust/apps/api-server/tests/records_http.rs` add `recovery_replays_after_completion_and_keeps_original_date`.
- Add `recovery_receipt_and_debt_projection_roll_back_together` to the provider target proposed in [package 47](subsidiary-journal-posting-receipts.md), including competing receipts.

Finish when an auditor can trace the exact recoverable amount from the write-off and distinct receipt records.
