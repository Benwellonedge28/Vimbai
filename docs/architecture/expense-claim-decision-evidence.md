# Work package 81 — Record expense-claim decisions once with an authenticated reviewer

Claims baseline [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Current update mechanism

`approve_expense` and `reject_expense` in `rust/crates/domains/payables/src/expenses.rs` patch status through `moved_to`. Approval accepts an approver string, and a second decision can overwrite the first. `expense_summary` uses the capped claim listing, so its counts can describe a page instead of the full company.

## Guard the decision

A submitted claim is pending and belongs to its company and Book. A versioned approval or rejection requires the expected claim revision and a scoped operation key. Derive the reviewer from the signed-in principal, require the Book's approval permission and refuse self-approval. Rejection requires a nonblank reason. Within one transaction append an immutable decision and update the claim's current status. An exact replay returns the same actor, time and result. An opposing outcome for a decided claim returns a conflict. Reconsideration creates a new claim revision linked to the prior decision with a reason and returns the record to pending. Approval does not record reimbursement, make a transfer or post a journal.

An exact Book-currency summary filters by company, window and state before aggregating all included rows. It can return a paginated list separately. A malformed legacy amount remains visible with an incomplete summary, rather than contributing zero. For multi-currency claims, group by their recorded currency unless a reviewed translation basis is provided.

## Caller and migration details

Update the records adapters calling `payables::expenses::approve_expense`, `reject_expense`, and `expense_summary` in `rust/apps/api-server/src/records.rs`. Keep legacy response fields and add a verified decision reference on opted-in claims. Older approvals and rejections retain their stored actor label as historical metadata. Do not authenticate a historical reviewer by matching a display name. A verified reconsideration can refer to that earlier decision without changing it.

Show “Another reviewer has already decided this claim” after a race, “A different authorized reviewer must decide your claim” for self-approval and “Explain why this claim is being rejected” for missing reason.

## Acceptance

- In `rust/apps/api-server/tests/payables_http.rs`, add `expense_decision_uses_authenticated_actor_and_checks_revision`.
- Add `expense_summary_counts_matching_claims_beyond_first_page` to that suite.
- Add `concurrent_claim_decisions_leave_one_durable_outcome` under the provider target in [package 47](subsidiary-journal-posting-receipts.md), including retry after interruption.

Release when a claim decision survives independently of a mutable display name and the summary reconciles to the complete selected claim set.
