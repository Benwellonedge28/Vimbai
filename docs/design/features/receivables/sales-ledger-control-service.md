# sales-ledger-control-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/receivables`.

- `GET /sales-ledger-control/control-account/summary`
- `GET /sales-ledger-control/debtors`
- `GET /sales-ledger-control/debtors/{}/balance`
- `GET /sales-ledger-control/health`
- `POST /sales-ledger-control/reconcile`
- `POST /sales-ledger-control/transactions`

## Functional design
Maintain customer obligation detail linked to posted control-account movements. Allocate receipts and credits explicitly; reconcile aggregate outstanding balances against general ledger at same cutoff. Differences produce exceptions, not automatic plugs.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Partial receipt, credit note, duplicate allocation, unapplied receipt and control reconciliation. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
