# Work package 84 — Produce supplier due plans using an actual as-of cutoff

Due-plan source review [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Labels and selection

`due_plan` in `rust/crates/domains/payables/src/purchases_ledger.rs` uses the requested day to assign due buckets but replays all supplied events. `due_plan_of_book` hands it a capped collection. A future payment can reduce today's open amount. An undated bill is already presented separately.

## Define the horizon and the allocation

The versioned read takes an as-of day and a Book-configured or explicitly requested nonnegative planning horizon. Enforce one Book and currency or group results by currency. Read all events effective through the cutoff, using one consistent snapshot and filtering before applying page limits. An invalid effective date blocks a verified total until reviewed. An unknown due date on an otherwise valid bill stays in the undated bucket. Order payments and credit adjustments chronologically. Apply an explicit supported bill reference first, and allocate unreferenced reductions oldest bill first only if the Book authorizes that method. Keep residual credit separate. A reference to a later bill remains unapplied on the earlier cutoff.

Buckets are overdue when due before the as-of day, due soon from that day through the horizon inclusive, and later after the horizon. Sum exact amounts and confirm total open bills equals the sum of all four buckets before publishing verified totals. Sort suppliers by descending overdue amount, then earliest known due date, descending amount and stable supplier ID. Expose configured horizon and cutoff in responses. A plan never invokes a payment or touches postings.

## Compatibility and review

Update `due_plan`, `due_plan_of_book` and the adapter calling the latter in `rust/apps/api-server/src/records.rs`. Preserve the legacy response while adding allocation warnings and currency to the versioned view. Keep existing entries as evidence. Disputed effective dates or unresolved bill references require review and must retain their prior source identity. No migration edits payment dates based on import time.

For a bad day say “Enter the date you want this supplier plan to describe.” If amounts cannot reconcile say “Some supplier movements could not be allocated as of this date.” Name reviewable records without exposing any from another Book.

## Proof

- Add `due_plan_ignores_future_bill_and_payment_events` and `due_plan_keeps_unapplied_credit_and_unknown_due_date_separate` in the purchases-ledger tests.
- In `rust/apps/api-server/tests/payables_http.rs` add `due_plan_window_and_ties_are_stable_for_complete_supplier_history`, testing exactly the horizon boundary and a multi-page history.

Accept when a saved payment dated tomorrow cannot change today's plan and all open amounts are accounted for by the published buckets.
