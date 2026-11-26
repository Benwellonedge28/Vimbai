# Work package 31 — Protect doubtful-debt provision adjustments from concurrent changes

Source baseline `e85043f61d13a2bef8f0d9fcdc066c559c09c3e6`. Proposed design.

## Current behavior

In `rust/crates/domains/accounting/src/doubtful_debts.rs`, `provide` reads the current allowance via `standing`, computes the difference to the requested provision, posts it, then remembers the result. The day-based reference constrains equal dates, but requests for different dates can both calculate their movement from the same prior balance.

## Conditional adjustment

Add optional `expected_standing` and `adjustment_id` to the existing command. The client supplies both for reviewed changes. At commit, resolve the Book's allowance and expense accounts and lock the allowance balance for the Book. Read standing and compare to `expected_standing` if supplied. For older clients without it, still derive the movement from the balance under the commit guard. If the reviewed value differs, refuse with “The provision changed since you reviewed it. It now stands at [amount]. Review it before adjusting.” Leave the requested target in the form. Retain current positive target validation and existing no-change behavior for a fresh command.

Compute the precise difference with Money. Raising debits expense and credits allowance. Lowering swaps the sides. Commit the provision receipt with its journal. An identified replay returns the stored starting and target amounts, journal ID and net movement. A reused ID with changed input refuses and asks the caller to start a new adjustment. Retain the existing day-based posting reference in this package. A distinct adjusted target already recorded for the same day follows the existing duplicate-day restriction and directs the caller to the recorded journal. Enforce the open-period and Book checks for every new adjustment.

## Paths and migration

Change `provide` and `list_provisions` in the same domain module. Extend the route in `rust/apps/api-server/src/accounting_book.rs` through its existing payload and error envelope. In `apps/flutter/lib/features/doubtful_debts/data/doubtful_debts_posting.dart` and `apps/flutter/lib/features/doubtful_debts/presentation/provision_page.dart`, preserve the draft on conflict and refresh the displayed standing before another confirmation. Provide the read-only current standing through the existing provision read surface as an additive response contract, or a separate scoped query if that response is an array consumed by older clients.

Existing provisions keep their references. New receipts record the standing used and the result. For historical receipt gaps, report a journal requiring reconciliation rather than calculating an assumed historical balance from today's chart.

## Verification

- `different_dated_provision_requests_cannot_use_the_same_standing` races two reviewed requests on both providers, and checks the rejected request wrote nothing.
- `provision_receipt_failure_leaves_standing_unchanged` injects a write fault after posting and verifies the previous balance after restart.
- `provision_retry_returns_original_movement_after_later_adjustment` verifies matching replay and mismatched ID refusal.
- `provision_conflict_preserves_the_requested_target` checks the Flutter state and renewed review.

Accept when each committed movement reconciles to the corresponding allowance change and provider tests prove the same conflict behavior.
