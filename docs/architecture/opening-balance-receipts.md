# Work package 22 — Recoverable opening balance receipts

Source baseline `42a2d2f81c94b14394f86169429be9cef1339b89`. Proposed design.
| Current path | Change |
| --- | --- |
| `rust/crates/domains/accounting/src/opening_balances.rs`, `post_opening_balances` and `opening_balances` | Commit the opening receipt with its ledger posting. Today the call to `remember` follows `post_lines`, so a receipt write error can follow a posted entry. |
| `rust/crates/adapters/database/src/lib.rs`, `Graph` | Provide one scoped transaction for the receipt and its posting. Reuse the ledger transaction contract specified in [posting account resolution](posting-account-resolution.md) where it can carry this receipt. |
| `rust/apps/api-server/src/accounting_book.rs` and `apps/flutter/lib/features/opening_balances/data/opening_balances_posting.dart` | Preserve the request and response shapes. Treat a matching retry as the original success, so the first-day form can recover after a lost response. |

## Write and replay
Resolve the requested chart rows in the Book and validate the date, account types, and exact balancing before starting the commit. Keep the fixed opening journal reference unique within the Book. Canonicalise the posting date, notes and account lines with normalised decimal values and deterministic line ordering. Persist a fingerprint alongside the receipt. Exclude server timestamps. At the transaction boundary, read the existing opening state before applying a new posting. Return the original receipt for a matching fingerprint. For a different fingerprint return “This Book already has an opening set. Review its journal before recording a correction.” A failed transaction leaves both records absent. Any correction uses the ledger reversal path. Preserve the current balance-sheet-account restriction and Book currency.

## Existing Books
Use existing receipts as-is and derive a retry fingerprint from their stored lines, date and notes when these are complete. If the opening journal exists alone, reconcile receipt metadata against the same Book, journal reference, module, canonical date and posted lines. Recover the receipt only from supported evidence and identify reconstructed metadata. Where evidence is ambiguous, show “An opening journal exists, but its receipt could not be verified.” Never issue a second journal to repair a missing receipt.

## Verification
- Add `opening_receipt_failure_rolls_back_the_posting` to provider conformance. Inject failure after the journal write on SQLite and PostgreSQL and verify neither record is visible after restart.
- Add `opening_retry_replays_receipt_and_other_payload_refuses` to `rust/apps/api-server/tests/accounting_end_to_end_http.rs`. Use concurrent matching requests, a changed effective date, and another Book. Assert one journal per Book and unchanged trial balance on refusal.
- Add `legacy_opening_receipt_recovery_requires_matching_journal` to the opening balance module. Exercise matching and mismatched legacy evidence.

Accept when list and post return the same journal ID after a dropped response, and a provider without atomicity fails before posting.
