# Work package 83 — Explain each movement in the purchases control account

Purchase-control source reviewed at [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Replay and summary count different things

In `rust/crates/domains/payables/src/purchases_ledger.rs`, `movement` gives refunds a negative effect, while `purchases_control_summary` reports only invoices, debit notes and payments. Unrecognized types have zero effect during replay. An unreadable stored amount in the summary can become zero, and an invalid cutoff is parsed like an omitted cutoff.

## Typed, dated accumulation

Add a versioned ledger-event schema for purchase invoices, debit notes, recorded payments and externally evidenced supplier refunds. Model refunds as distinct events with explicit Book-owned movement semantics. A new refund specifies whether it reverses a payment and restores the payable, or settles a supplier credit by increasing the net payable from a credit balance. Both supported cases increase the signed payable and require original evidence. Keep the existing negative refund sign only when replaying a legacy record. A discount already included in an invoice or debit note cannot be deducted again as a standalone line. Unsupported types require correction before saving.

Read each supplier precisely once from a complete Book-and-currency cohort. Apply a verified opening checkpoint followed by exact dated deltas. Use one reducer to produce each supplier balance and the breakdown of movements for the control view. Explicitly distinguish positive payable from supplier credit. Process dated movements through the requested calendar day inclusive and sort same-day events by stable record identity. Report unreadable legacy rows as incomplete and retain identified source rows for review. Stored historical signs remain visible and no sign is silently rewritten.

## Caller contract

Use the typed reducer from `record_transaction`, `creditor_account`, `purchases_control_summary` and `purchases_reconcile`. Carry the opt-in cutoff and classification result through `rust/apps/api-server/src/records.rs`. Keep older response keys. The original comparison remains a detail consistency check. A verified accounting reconciliation also identifies separately read posted control evidence, following the [sales-control basis](sales-control-reconciliation-basis.md). Versioned purchase movements change only the subsidiary record. Journal changes use accounting's posting or reversal path.

Display “Enter a valid as-of date” for an invalid cutoff, “Identify the recorded payment or credit this refund relates to” for missing evidence and “These older transactions need review before the total can be verified” when history is incomplete.

## Checks

- In the purchases module add `purchase_refund_is_explained_in_summary_and_replay` with both legacy and verified signs.
- Add `purchase_summary_rejects_bad_cutoff_without_leaking_later_rows` to `rust/apps/api-server/tests/payables_http.rs`.
- Add `multiple_creditor_pages_reconcile_to_exact_movement_totals` to the HTTP suite, with an unknown legacy type and a foreign supplier ID.

Accept when the published chronological rows reconcile to the signed supplier and control totals without treating unreadable history as zero.
