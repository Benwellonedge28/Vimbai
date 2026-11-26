# Work package 94 — Reconcile partnership dissolution figures to their source evidence

Dissolution source pinned at [5c2d0018](https://github.com/Benwellonedge28/Vimbai/commit/5c2d0018ed10d212ab43eda94a0a8d2509c3ea0d).

## Where the current report assumes completion

`dissolve` in `rust/crates/domains/partnerships/src/dissolution.rs` defaults omitted creditor payments to the amounts owed, accepts separately entered discounts and partner result shares, and saves the report as completed. Asset realization, partner claims and external cash paid can therefore describe different evidence sets.

## Dated reconciliation worksheet

A verified worksheet requires an authorized agreement revision, cutoff, currency, opening cash, and unique scoped asset, creditor and partner IDs. Record net proceeds and carrying values by realized asset, exact realized gain or loss, and supporting references. A creditor line separates prior liability, evidenced payment, approved discount and unpaid remainder. Require all nonnegative components and `paid + discount + unpaid = amount owed`. Missing payment is unknown and cannot become paid. Compute the signed realization result from net asset gains less losses, plus verified creditor discounts less separately evidenced realization costs. Allocate that result using effective agreement ratios with largest-remainder rounding. The allocations must sum to the exact result, and a negative result reduces the partner claims. A claim is signed capital plus current balance plus its allocated result.

Compute observed cash as opening cash plus realized cash proceeds less evidenced expenses and creditor payments. Expose amounts available and claims separately. Completion requires all evidence and an exact explanation of any residual cash or unpaid liability. A report with missing proof or unresolved differences is saved as incomplete. This worksheet cannot distribute cash, discharge creditors or post a journal. Save the source revision and totals once per scoped report key, using an attributed new revision for corrections.

## Delivery and older reports

Update the adapters calling `dissolution::dissolve` and `get_dissolution` in `rust/apps/api-server/src/records.rs`. Keep legacy fields and distinguish verification and completion on the versioned read. Preserve older reports and their original status. Never infer an actual settlement from a default-filled paid value.

Say “Record evidence of what was actually paid to this creditor” for missing payment, “The recorded creditor figures do not reconcile” for inconsistent components and “The realization result does not match the agreed sharing terms” for invalid allocations.

## Acceptance

- In the dissolution module add `unobserved_creditor_payment_keeps_report_incomplete`.
- Add `dissolution_gain_or_loss_allocates_exactly_to_partner_claims`, including unequal ratios and a final minor-unit remainder.
- Add `dissolution_report_retains_legacy_history_and_respects_book_scope` to `rust/apps/api-server/tests/records_http.rs`.

Done when a reviewer can reconstruct each liability, cash figure and partner amount from the saved evidence.
