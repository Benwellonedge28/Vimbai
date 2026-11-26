# Work package 92 — Preserve the membership and terms of each partnership agreement revision

Agreement source reviewed at [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Existing edits

`update_agreement` in `rust/crates/domains/partnerships/src/agreement.rs` copies fields whose names already exist, with a short denylist. This allows direct edits to either partners or derived capital. `add_partner` appends to the array and increments the stored capital. It does not reserve partner identity or compare an expected revision.

## Effective membership and exact totals

Give each agreement a stable Book-scoped ID and append-only revisions. Separate editable descriptive terms from financial membership and share terms. A new membership revision validates a unique stable partner ID, eligible effective date, nonnegative exact contribution in Book currency and approved sharing fractions whose sum is exactly one. Preserve partner names for display separately. A new contribution or correction records its author and basis. Derive capital from the entire validated cohort, never from the submitted total or the prior cache. Keep the effective date and actor on every revision.

Reserve one successor to the expected prior revision in the same transaction that saves the new revision. A matching operation key returns its first result before checking the current state. Require an authorized amendment and reason when correcting an agreement with downstream references. The latest pointer changes only after the new revision is durable. A reader requesting an earlier agreement date resolves the applicable revision and returns its ID. No agreement change posts capital or recasts journals.

## Callers and legacy migration

Update `update_agreement`, `add_partner`, `get_agreement` and `agreement_summary`, plus the corresponding handlers in `rust/apps/api-server/src/records.rs`. Preserve legacy response keys. Versioned rows accept only typed amendments. Convert older agreements to an initial verified revision only once partner identities, contributions and shares reconcile. Retain unreadable inputs or duplicate partner labels as legacy review items.

Say “This partner already belongs to the agreement” for a duplicate, “The sharing fractions must total 100%” for a mismatch and “Open the latest agreement before making another change” for a stale revision.

## Checks

- Add `agreement_revision_derives_capital_from_unique_partners` and `agreement_shares_reconcile_at_exact_precision` to the module tests.
- In `rust/apps/api-server/tests/records_http.rs`, add `agreement_as_of_read_keeps_previous_member_and_terms`.
- In the provider target proposed in [package 47](subsidiary-journal-posting-receipts.md), add `concurrent_partner_admissions_preserve_one_successor`.

Done when historical reports can reopen the member terms that were effective, and the agreement's stored capital is independently reproducible.
