# subscription-plans-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/invoicing`.

- `GET /subscription-plans/health`
- `GET /subscription-plans/plans`
- `GET /subscription-plans/subscriptions/{}`
- `POST /subscription-plans/plans`
- `POST /subscription-plans/subscribe`
- `POST /subscription-plans/upgrade`

## Functional design
Customer Book billing plans define recurring invoice terms, interval, price, currency and effective dates. Distinguish this feature from Vimbai software subscriptions. Worker schedules idempotent invoice generation; no automatic debit or Vimbai Pay.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Month-end recurrence, plan amendment, cancellation, duplicate job, missed run and invoice reconciliation. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
