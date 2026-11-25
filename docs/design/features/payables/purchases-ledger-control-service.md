# purchases-ledger-control-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/payables`.

- `GET /purchases-ledger-control/control-account/summary`
- `GET /purchases-ledger-control/creditors`
- `GET /purchases-ledger-control/creditors/{}`
- `GET /purchases-ledger-control/due`
- `POST /purchases-ledger-control/reconcile`
- `POST /purchases-ledger-control/transactions`

## Functional design
Track supplier obligations and allocations against configured payable control account. Vendor bill, credit and payment record remain linked; prevent double settlement and preserve unapplied amounts. Reconcile at consistent cutoff.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Partial settlement, supplier credit, duplicate bill, FX difference and control-account mismatch. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
