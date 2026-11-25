# revenue-recognition-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/accounting`.

- `GET /revenue-recognition/contracts/{}`
- `GET /revenue-recognition/summary/{}`
- `POST /revenue-recognition/contracts`
- `POST /revenue-recognition/contracts/{}/recognize`

## Functional design
Recognition schedules link contract, performance obligation, allocated consideration, satisfaction evidence and policy version. Distinguish invoice, receipt and recognized revenue; recognize only approved amounts and dates. Contract modifications and cancellations produce auditable adjustments.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Partial satisfaction, deferred revenue, modification, duplicate recognition, period closure and schedule-to-ledger reconciliation. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
