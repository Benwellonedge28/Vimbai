# Approve and account for one department’s budget commitments

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Requires 02, 03 and 10. The first scope is one department operating under a reviewed budget authority.

## Reusable code and ownership

`rust/crates/domains/accounting/src/fund_accounting.rs` owns fund records and fund transactions. `rust/crates/domains/npo/src/npo.rs` includes restricted funds and budget lines. `rust/crates/domains/budgeting/src/budget_register.rs` exposes the budget register. Use these for existing concepts and review how they relate to Book postings. A statutory appropriation needs a separately owned version and lifecycle. Extend the budgeting domain with an application module for that path. Keep ledger posting in the existing ledger authority and keep fiscal approvals in the workflow and authorization boundary. No government-wide chart, ruleset or cash-management integration is required to start.

## Budget and commitment lifecycle

A budget authority records entity, fiscal year, approval evidence, official classification version and allocations. Each allocation is keyed by authority, fund, programme, economic category and period. Labels may change in a new version without changing historical IDs. Enforce positive exact-money ceilings in the Book currency. A transfer has proposed, approved and applied states and conserves the amount between approved source and destination lines. Authority changes affecting the total carry a separate version and source authorization.

A commitment starts as requested. Approval reserves against an allocation atomically. A recognized invoice consumes the commitment amount and posts through the normal payables path with an evidence link. Releasing or cancelling a remaining obligation returns only the unused reservation. Define available as approved allocation minus open reservations and recognized actuals. Reconcile both values after every state change. An invoice for 40 against a reservation of 100 leaves 60 reserved and 40 actual, without reducing availability twice. A close report pins a classification version and shows allocation, outward and inward transfers, open commitments, actuals and available authority. Require a separate user for preparation and approval on the pilot. Read-only auditors can see the evidence and figures scoped to their own Book.

## Boundary, migration and errors

Existing Book balances and budget lines remain usable. Pilot adoption requires an opening appropriation schedule, opening commitment register, accountable reviewer and dated reconciled import. Existing transactional rows cannot be assumed to be commitments. Insufficient authority refuses the approval with the available amount and the named line. Missing law, evidence or policy gives a configuration error before any commitment is reserved. Keep payroll and identifiable supplier details out of public exports. Generate any public aggregate through an explicitly approved disclosure workflow.

## Acceptance tests

- `concurrent_approvals_cannot_overspend_an_allocation` checks two eligible requests racing for the same headroom.
- `invoice_consumption_moves_reservation_to_actual_without_double_counting` covers partial invoices and later releases.
- `an_unapproved_transfer_leaves_available_authority_unchanged` exercises segregation of duties and revision history.
- `department_and_public_reporting_respects_scope` checks tampered scope and excludes personal details.
- Done when the pilot department and its reviewer reconcile an opening schedule through one reporting period and verify that commitments, general ledger postings and the published aggregate can be traced to the approved version.
