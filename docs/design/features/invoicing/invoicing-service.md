# invoicing-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/invoicing`.

- `DELETE /invoicing/customers/{}`
- `DELETE /invoicing/invoices/{}`
- `GET /invoicing/customers`
- `GET /invoicing/customers/{}`
- `GET /invoicing/invoices`
- `GET /invoicing/invoices/{}`
- `POST /invoicing/customers`
- `POST /invoicing/invoices`
- `POST /invoicing/invoices/{}/record-payment`
- `PUT /invoicing/customers/{}`
- `PUT /invoicing/invoices/{}`

## Functional design
Compose customer, invoice, document rendering and receivable posting workflows using existing authorities. Avoid duplicate invoice stores across invoices and invoicing routes; inspect handlers to choose canonical ownership. Status reflects verified events rather than payment redirect.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Route parity, duplicate submission, renderer failure, retry, authorization and ledger reconciliation. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
