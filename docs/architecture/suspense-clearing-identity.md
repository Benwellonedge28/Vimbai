# Work package 23 — Identify each suspense clearing

Source baseline `42a2d2f81c94b14394f86169429be9cef1339b89`. Proposed design.
## Current risk
`clear_suspense` in `rust/crates/domains/accounting/src/suspense.rs` constructs a default reference from date, target account and amount. Direction and source evidence are absent from that reference. Two independent equal clearings can collide, and an ambiguous retry gives the user no verified receipt.

## Implementation
Accept an optional opaque `clearing_id` on the existing request. The Flutter client in `apps/flutter/lib/features/suspense/data/suspense_clearing.dart` generates it once for a submitted form and reuses it on timeout. `apps/flutter/lib/features/suspense/presentation/clear_suspense_page.dart` keeps that submission available until the server answers or the caller explicitly starts another clearing. The route in `rust/apps/api-server/src/accounting_book.rs` forwards it to `clear_suspense`, which binds it within the Book and suspense clearing operation to the canonical date, direction, both accounts, amount and reason. Keep any supplied human reference visible as evidence for new identified requests. Legacy requests retain their existing reference semantics.

For an identified request, store the fingerprint and receipt with the journal using the shared posting transaction. Return the same receipt on a matching retry, including if the month has since closed. Refuse a changed request using the same ID with “This clearing was already submitted with different details. Review the earlier entry or start a new clearing.” A retry still requires access to the Book. Reject missing configured suspense, same-account clearing and closed months for fresh entries using the existing messages. Use the same exact Money and debit/credit rules. Keep corrections as ledger reversals.

## Compatibility and proof
Existing suspense journals keep their original references. Do not infer IDs from their amount or date. The receipt adds the clearing ID only for the new path. An old client still receives the current fields.

- `equal_clearings_with_distinct_ids_both_post` in the suspense module posts opposite directions to the same account and day for equal amounts.
- `clearing_id_replays_only_its_bound_payload_and_book` exercises lost response, changed direction, closed month and cross-Book access through the API.
- `timeout_reuses_clearing_identity` exercises the captured requests in the Flutter suspense test.

Accept when two stores demonstrate one balanced journal per identified request and report the same journal ID for a retry.
