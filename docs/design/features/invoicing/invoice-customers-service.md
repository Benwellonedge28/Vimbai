# invoice-customers-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/invoicing`.

- `DELETE /customers/{}`
- `GET /customers`
- `GET /customers/{}`
- `POST /customers`
- `PUT /customers/{}`

## Functional design
Customer master is Book-scoped with stable identity, contact details, terms and tax attributes. Preserve issued invoice snapshots when customer details change. Merge duplicates only through approved mapping; restrict sensitive fields.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Duplicate customer, changed address, inactive customer, cross-Book lookup and historical invoice rendering. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
