# invoices-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/invoicing`.

- `DELETE /invoices/{}`
- `GET /invoices`
- `GET /invoices/discounts`
- `GET /invoices/{}`
- `GET /invoices/{}/payments`
- `POST /invoices`
- `POST /invoices/discounts`
- `POST /invoices/discounts/quote`
- `POST /invoices/{}/record-payment`
- `PUT /invoices/{}`

## Functional design
Draft, validate, issue, record settlement and correct through linked credit/debit documents. Exact line totals, taxes and discounts follow policy; numbering atomic. Issuance and posting coordinated with durable idempotency. No money movement.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Concurrent numbering, duplicate issue, partial settlement, credit note, closed period and immutable issued snapshot. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
