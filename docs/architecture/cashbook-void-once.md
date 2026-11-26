# Work package 32 — Apply a cashbook void exactly once

Proposed design against `dfd535cab7037f71de3c4ee1d1268b5c094f5683`.

## Current behavior
In `rust/crates/domains/accounting/src/cashbook.rs`, `void_entry` reads the entry, applies the inverse of its stored base amount with `move_balance`, then marks the entry voided. The function has no already-voided check. A repeat or an interruption after the balance movement can therefore change the balance twice.

## Transition
Within one Book-scoped store transaction, lock or conditionally update the entry and its money-account balance. Read the current entry state under that guard. For a standalone unreconciled entry, persist the inverse of its original signed base amount, the voided status and a receipt carrying entry ID, reason and authorised actor. Use stored Money and rate evidence. Do not revalue using today's exchange rate. A retry after commit returns the receipt. A different reason on a later request leaves the existing receipt intact and tells the user it was already voided. If the entry is linked to a ledger journal, stop and direct the user to the journal's reversal flow. If reconciled, direct the user to the reconciliation review. Neither refusal changes the balance. Check the Book and correction date before writing.

## Callers and rollout
Use the `accounting::void_cash_entry` call in `rust/apps/api-server/src/records.rs` and preserve the success fields `status`, `entry_id` and `reason`. Refresh the account from the server through `apps/flutter/lib/features/cashbook/data/cashbook_service.dart` after a confirmed void. For existing voided entries lacking receipts, report that the void already exists without another balance change. Flag discrepancies for review against source records. Keep the original cash row and journal untouched.

## Acceptance
- `cash_void_replay_applies_original_base_amount_once` uses a foreign-currency row and a changed current rate.
- `cash_void_failure_rolls_back_balance_and_status` injects failure between the two writes on SQLite and PostgreSQL.
- `cash_void_and_reconciliation_cannot_both_commit` races the transitions and verifies a consistent outcome.
- `legacy_voided_row_does_not_mutate_balance` verifies no second movement on upgrade.

Complete when two concurrent void calls and a restart leave one receipt and one inverse balance movement.
