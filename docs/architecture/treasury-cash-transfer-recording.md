# Work package 72 — Record an external cash transfer without splitting its two balances

Reviewed cash-management source at [3a2c2ca1](https://github.com/Benwellonedge28/Vimbai/commit/3a2c2ca16978df88a95ad6b1c1decb2148a8707b).

## Existing writes

`create_transfer` in `rust/crates/domains/treasury/src/cash_management.rs` reads both cash accounts from a capped Book list, applies two balance updates, then saves a transfer. The same account can be selected twice. Both patches use the original balance, so the second write can overwrite the first rather than netting to zero. A write failure can also leave a one-sided adjustment. The function changes treasury records. It does not instruct a bank.

## Record the completed event

For an evidence-backed transfer require distinct scoped account IDs, their expected balance revisions, the already-completed amount, currency, date, proof reference and operation key. Require positive exact `Money`, matching established account currencies and a valid effective date. Preserve possible historical transfers below a treasury minimum by recording the observed breach explicitly. The evidence-backed action records what occurred. A separate planning check may still refuse to recommend a transfer below minimum. If the source chronology cannot reconcile with its verified balance checkpoint, refuse until an authorized adjustment is reviewed.

Resolve accounts by ID and apply the debit, credit and `CASH_TRANSFER` receipt under one Book-scoped transaction. Lock both accounts in stable ID order and save their before and after balances. Combined balance in that currency is conserved. Serialize against other cash-account mutations, including balance edits through the shared cash-optimization record path. The operation key is unique within the Book. Replay returns the previous receipt before comparing current account revisions. Reuse with different facts returns a conflict. The receipt references any already-posted accounting evidence without writing it. Ledger balances can change only through the accounting authority.

## Route work and old rows

`move_cash` and `cash_transfers` in `rust/apps/api-server/src/records.rs` continue to serve the existing paths. Add an explicit versioned recording contract and expose the transfer proof to its callers. Prevent an unguarded legacy transfer from changing verified account balances. Preserve historical transfer IDs. Compare old balances with authorized external evidence and mark discrepancies for review. Repair by a recorded balance adjustment with a reason and before/after values. Never replay an old transfer just because one side is missing.

Show “Select two different treasury accounts” for a self-transfer, “Both accounts must use the same currency for this recording” for a currency mismatch, and “These balances changed. Review the latest amounts and retry” for a stale revision.

## Completion checks

- In `rust/apps/api-server/tests/records_http.rs`, add `external_transfer_rejects_same_account_and_keeps_book_currency`.
- In the provider target proposed in [package 47](subsidiary-journal-posting-receipts.md), add `cash_transfer_receipt_and_both_balance_changes_commit_together` and `cash_transfer_key_replays_after_concurrent_balance_change`. Check both providers and inspect the untouched ledger.

Accept when a reviewer can reconcile both account deltas to one receipt and interrupted commands never leave a partial adjustment.
