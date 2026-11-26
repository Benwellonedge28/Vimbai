# Work package 82 — Reconcile debtor movements to independent control-account evidence

Sales-control baseline [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Why today's comparison can mislead

`reconcile` in `rust/crates/domains/receivables/src/sales_ledger.rs` compares a replay of `DEBTOR_TRANSACTION` records with a control total computed from the same record kind. The detail replay includes refunds, but `control_summary` has no refund bucket. The Book collection is capped, and the result treats a difference under one cent as reconciled through a float comparison.

## Use independent balances for the same cutoff

For a versioned reconciliation require the effective cutoff, Book currency, a verified opening checkpoint and the Book's configured receivables control account. Resolve the control account using the [posting-account resolution design](posting-account-resolution.md). At the application boundary read the posted control balance through the existing accounting ledger reader, excluding unposted entries and applying dated reversals. In the receivables service, replay signed debtor movements from the checkpoint through the cutoff. Carry a common store read point across both sides or explicitly identify that a consistent snapshot was unavailable. Scope each read before pagination. Malformed amount, unknown movement or unresolved historical date makes the comparison incomplete. A separately stored legacy balance in a detail row is evidence for review, not an extra movement.

Classify the result as matched, difference or incomplete. Matching means exact equality at Book precision. A signed difference is posted control less the sum of debtor details. Return the cutoff, both evidence identities and missing row IDs, with null verified difference if a side is incomplete. An itemized debtor result and the same set's summary derive from one typed movement table, including the existing refund sign when reading legacy records. The comparison cannot create a balancing journal or change a closed period.

## Delivery and historical rows

Update `control_summary`, `debtor_balance` and `reconcile` in the receivables module and their adapters in `rust/apps/api-server/src/records.rs`. Keep the legacy self-comparison available during migration with its original response fields. Historical rows keep their IDs and amounts. Verify or replace the opening checkpoint under review before certifying a period. A mismatch says “Debtor detail differs from the posted receivables account.” An incomplete comparison says “Resolve the listed debtor records before relying on this balance.”

## Acceptance

- Add `sales_control_comparison_uses_posted_ledger_not_its_own_detail_total` to `rust/apps/api-server/tests/records_http.rs`, deliberately varying one side.
- In the domain tests add `sales_control_includes_replayed_refunds_and_never_reports_partial_match`.
- Add `sales_reconciliation_cutoff_and_book_scope_cover_multiple_pages` to the HTTP suite.

Accept when exact equality is established between separately sourced amounts at a reproducible cutoff.
