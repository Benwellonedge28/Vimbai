# Work package 33 — Complete cashbook reconciliation atomically

Proposed design against `dfd535cab7037f71de3c4ee1d1268b5c094f5683`.

## Current behavior
`complete_reconciliation` in `rust/crates/domains/accounting/src/cashbook.rs` records completion before marking linked entries. It skips entries that fail lookup. An error can leave a completed reconciliation with missing links.

## Commit contract
Use the reconciliation ID in the active Book. Resolve every explicit entry reference against the same Book and money account. Preserve adjustments with no entry reference as standalone evidence. Deduplicate repeated IDs for linking while retaining the original adjustment rows. Refuse a missing, voided or differently reconciled entry. Use “A listed entry is unavailable or belongs to another reconciliation. Review the selected entries before completing.” Under a single transaction, validate link ownership, write all links and persist completion evidence. An exact retry returns the original actor and completion time. A completed reconciliation cannot be reassigned by retransmission. The transition changes matching evidence only. Its supplied statement amount, currency and ledger journals are unaffected.

## Read and upgrade
Keep the `complete_reconciliation` handler in `rust/apps/api-server/src/records.rs` and its current success shape. In `apps/flutter/lib/features/cashbook/presentation/reconcile_sheet.dart`, preserve the selection on refusal and refresh it after success. Use `outstanding_items` and the existing read route to consume the same verified links. For legacy completed records check each referenced entry; indicate “Completion needs review” when links are missing or conflicting. A reviewer can repair links only after verifying ownership and current state. Preserve the old completion evidence.

## Acceptance
- `complete_reconciliation_rolls_back_on_last_link_failure` verifies no partial status or links on both stores.
- `two_reconciliations_cannot_claim_one_cash_entry` verifies the loser produces no writes.
- `legacy_completed_reconciliation_with_missing_entry_is_flagged` keeps the original record discoverable.
- `completed_reconciliation_retry_preserves_review_evidence` verifies replay and unchanged displayed totals.

Complete when the completion receipt, linked entries and outstanding-items response agree after restart.
