# Work package 24 — Make stock count adjustments recoverable

Source baseline `42a2d2f81c94b14394f86169429be9cef1339b89`. Proposed design.
## Current workflow
`rust/crates/domains/accounting/src/stock_adjustments.rs::adjust` posts with a day/item reference and subsequently calls `remember`. `list_adjustments` reads those receipt rows. A failed receipt write can therefore leave a journal absent from the stock count list. A display label also cannot distinguish two independent counts of the same stock.

## Changes
Add optional `count_id` to the existing command forwarded by `rust/apps/api-server/src/accounting_book.rs`. Use a Book-scoped opaque ID. Keep the item label for display. For an identified count, bind the date, item, direction, exact amount, resolved inventory and adjustment accounts, reason and notes to the ID. Keep the identifiers and resulting journal link in a receipt committed in the same transaction as the ledger post. Reuse the shared posting transaction. A repeated ID with equal canonical input returns the existing receipt. Changed input returns “This count has already been recorded with different details.” The month check applies to fresh writes. Use a permitted reversal for a correction and retain the original count evidence.

In `apps/flutter/lib/features/stock_adjustments/data/stock_adjustments_posting.dart` and `apps/flutter/lib/features/stock_adjustments/presentation/stock_count_page.dart`, allocate the ID before the first request, reuse it for a retry, and render the journal reference on success. If the outcome is unknown, keep the submitted details visible. Starting another count creates another ID. A shortage remains debit adjustment and credit inventory. A surplus reverses those sides. Keep inventory mappings within the current Book.

## Rollout and evidence
Keep legacy stock references and receipt rows readable. Old requests use their current day/item identity. For a journal with no receipt, surface a reconciliation item to authorised operators. Recover only fields proved by the journal. Flag missing count evidence explicitly. An ambiguous record requires operator resolution and cannot generate a second posting automatically.

- `stock_count_retry_does_not_change_inventory_twice` covers a dropped response and a conflicting payload.
- `count_receipt_failure_leaves_no_inventory_adjustment` injects storage failure on both supported stores.
- `separate_counts_for_the_same_label_have_separate_receipts` proves same-day counts can be distinguished while filtering still uses the item label.
- `stock_count_timeout_keeps_the_submission_identity` verifies the Flutter retry behavior.

Accept when the stock list and journal can be reconciled through persistent receipt IDs after restart.
