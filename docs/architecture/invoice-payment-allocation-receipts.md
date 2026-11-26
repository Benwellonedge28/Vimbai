# Work package 79 — Allocate a recorded invoice receipt once against the net balance

Source baseline for receipt allocation [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Current gap

`record_invoice_payment` in `rust/crates/domains/invoicing/src/invoices.rs` creates a payment row before updating the invoice. It compares paid amount with `total_amount` rather than net amount due. `invoice_payments` filters a capped Book collection after reading it. The code records payment evidence and explicitly reports no ledger posting.

## Exact allocation

The versioned command takes an issued invoice revision, external receipt reference, signed-in recorder, received date, currency and positive exact amount. Support Book-currency invoices first. Resolve the unique invoice in its Book and derive its available net balance from the frozen net due and prior verified allocations. Refuse an allocation beyond the balance without silently storing a negative outstanding value. Report excess as an amount requiring separate accounting review. A receipt referencing an existing journal must resolve that journal through the accounting application boundary and verify amount, customer and link context. The receipt itself writes no ledger entry.

Reserve one canonical allocation operation per Book and invoice. Compare the request digest before reading current balance, so an exact replay still returns success after the invoice becomes paid. Insert the receipt and advance invoice paid projection and status in one scoped transaction. A concurrent operation locks the same invoice revision and rechecks outstanding at commit. Preserve distinct partial and paid states, and report as-of balances using receipt effective dates. An authorized correction records an inverse allocation referencing the original receipt. Accounting handles any required journal reversal.

## Ship and migrate

`post_invoice_payment` and `get_invoice_payments` in `rust/apps/api-server/src/records.rs` preserve existing routes. Add receipt IDs and net-balance fields for versioned records and page directly by invoice identity. Read preexisting payments, flag ambiguous invoice numbers and recompute a migration projection only if the complete payment evidence agrees with the frozen net amount. Never issue an extra payment to repair a damaged projection.

Say “The receipt exceeds the outstanding amount on this invoice” for excess, “This reference already identifies a different receipt” for changed replay and “Review the earlier receipt records before recording another” when history cannot be verified.

## Acceptance

- In the invoice tests add `discounted_invoice_settles_when_exact_net_is_received`.
- Add `invoice_payment_history_filters_by_invoice_before_paging` in `rust/apps/api-server/tests/records_http.rs`.
- Add `racing_invoice_receipts_cannot_overallocate_or_leave_partial_projection` to [package 47](subsidiary-journal-posting-receipts.md), including write failure and exact replay after restart.

Done when the net balance reconciles to individual recorded receipts on both stores.
