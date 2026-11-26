# Work package 28 — Keep one prepayment release receipt per month

Source baseline `e85043f61d13a2bef8f0d9fcdc066c559c09c3e6`. Proposed design.

## Current behavior

`release_prepayment` in `rust/crates/domains/accounting/src/accruals.rs` reads a `released` array, posts the scheduled share, and updates the parent with the array and a single `released_entry_id`. Concurrent releases for different periods can overwrite the parent projection, and the single ID only identifies the most recent stamped release.

## Persistence and operation

Add Book-scoped release receipts keyed by prepayment ID and schedule period. Store the original schedule amount, entry date, expense and prepaid account references and resulting journal ID. Before a fresh release, validate the schedule, current Book and open period. Commit the receipt and balanced Dr expense / Cr prepaid entry in the posting transaction. If a receipt already exists, return the existing receipt without entering the posting path. Derive released periods and remaining balance from verified receipts plus explicitly identified legacy markers. Calculate the balance using exact Money, and use the existing wire format at the route boundary.

Return “This prepayment does not cover [period]” for unscheduled months. A closed fresh month keeps the existing refusal. When the receipt and journal disagree, show “The release for [period] needs journal review” and reserve that month from new release. A failed receipt transaction leaves the schedule available for retry.

## Callers and upgrade

Change `release_prepayment` and `list_prepayments` together. In `rust/apps/api-server/src/accounting_book.rs` preserve route and response fields, and add an ordered `release_receipts` field. The legacy single journal field remains a projection of the latest completed release, using persisted order. In `apps/flutter/lib/features/accruals/data/accrual_posting.dart` and `apps/flutter/lib/features/accruals/presentation/paid_up_front_page.dart`, show each month's verified receipt independently.

For old released markers, use the current journal reference formed from prepayment reference plus period. Confirm scope, exact date, Money and both accounts. Unverified markers remain visible as such and keep their months reserved until cleared by authorised reconciliation. Do not invent journal IDs.

## Proof

- `different_months_released_concurrently_preserve_both_receipts` uses independent SQLite and PostgreSQL handles.
- `prepayment_release_retry_returns_original_journal_after_period_close` exercises the existing HTTP adapter.
- `legacy_release_marker_without_matching_lines_needs_review` covers legacy reconstruction and refusal.
- `release_remaining_amount_preserves_the_schedule_total` proves exact Money across multiple releases including the final rounding remainder.

Accept when two concurrent releases leave two reachable journals, and replay after restart leaves the prepaid balance unchanged.
