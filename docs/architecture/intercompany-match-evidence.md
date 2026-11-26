# Work package 49 — Verify intercompany pairs before recording eliminations

Design baseline [2c0642c](https://github.com/Benwellonedge28/Vimbai/commit/2c0642cd994674e4d455e93d524031eb6477e9a1).

## Existing match

`match_transactions` in `rust/crates/domains/accounting/src/intercompany.rs` tests exact numeric equality, writes two reciprocal pointers in separate updates and creates an elimination afterward. It accepts the same direction, different currencies and reuse of an already matched leg. The unit test `two_legs_in_the_same_direction_still_match_and_eliminate` pins that behavior. `create_transaction` currently accepts the supplied entity IDs without resolving them. `match_intercompany` in `rust/apps/api-server/src/records.rs` serves both the primary and `/records` match routes.

## Prepare a pair and apply it once

Add a verified-match command alongside the existing matcher. The caller provides two distinct transaction IDs and identifies which is the debit leg for the working paper. Resolve both transactions and both legal entities in the active Book. Require reciprocal entity IDs, the same transaction type, the same explicit currency and the same positive exact amount. The reviewer must affirm the economic link by supplying an evidence reference, such as a shared invoice or a reconciliation working paper. For a legacy leg with no proof that its stored currency was actually supplied, require the reviewer to confirm the currency against the attached evidence. A decimal equality alone never supplies that reference. Keep debit and credit entity IDs from the opposing legs, transaction type, exact amount and currency, both source IDs, evidence reference and authenticated reviewer in the elimination.

Within a single store transaction, load both legs in a stable ID order. Compare their current versions and reserve both legs for the same elimination. Key the elimination by Book scope and the sorted pair of full IDs. Derive its debit and credit from the explicit orientation, independently of request array order. Return the existing elimination for a replay with the same orientation and evidence. Return a conflict when the evidence or orientation changed, or when either leg already belongs to another elimination. Store the reciprocal pointers and elimination in the same transaction. A competing writer retries the read and returns either the original result or the conflict. The operation produces working-paper evidence only. Each underlying journal remains under its own Book's authority, and [cross-Book consolidation](../domains/cross-book-consolidation.md) retains its own mapping, currency and approval gates.

## Delivery and review of historical matches

Expose `POST /intercompany/transactions/verified-match` and its corresponding records alias through `records.rs`. Register the route in the existing contract inventory using the catalogue and route generation workflow. Keep the current success keys `matched` and `elimination_id` on the new command. Apply ADR-004 to migration of the existing match path. While older clients can still write through that path, mark its eliminations as legacy and make every writer consult the same Book-scoped leg reservation. The verified-elimination view includes verified rows by default and reports the presence of older, unreviewed working papers separately. Keep all existing IDs and pointer values for audit.

Backfill a verification record only after review of complete transaction and entity evidence, including reciprocal pointers and exclusive leg use. A partial pointer, reused leg, ambiguous orientation or missing currency creates a review item keyed by the affected IDs. Never infer an exchange rate or create a new accounting posting during backfill. Until the review completes, exclude those papers from any verified consolidation input.

| Refusal | Display text |
| --- | --- |
| Missing or inaccessible leg | “Choose two transactions you can access in this Book.” |
| Same leg, invalid counterparties, amount or currency | “These transactions do not form a matching pair. Check their entities, type, currency and amounts.” |
| Missing evidence | “Add the reference that shows these records belong to the same transaction.” |
| A competing elimination already owns a leg | “One of these transactions is already matched. Open the existing elimination.” |
| Storage failure | “The elimination was not saved. Retry the same match.” |

## Proof to add

- In `intercompany.rs`, add `verified_pair_refuses_same_direction_and_equal_amount_in_different_currency`. Hold the amounts equal to isolate each reason.
- In the accounting provider integration target from [package 47](subsidiary-journal-posting-receipts.md), add `one_intercompany_leg_cannot_be_reserved_by_two_eliminations` and `intercompany_second_pointer_failure_rolls_back_both_legs`. Run them on SQLite and PostgreSQL.
- In `rust/apps/api-server/tests/records_http.rs`, add `verified_intercompany_retry_reorders_ids_without_changing_orientation`. Replay on the records alias and exercise changed evidence and an inaccessible Book separately.
- Add `old_intercompany_pointer_without_elimination_requires_review` to the domain tests. Verify the original row survives verification refusal.

Done when the working paper cites the two exact source records and their evidence, and neither a race nor a partial failure gives a leg two verified owners.
