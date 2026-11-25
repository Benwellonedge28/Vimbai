# expense-tracking-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/payables`.

- `GET /expense-tracking/expenses/{}`
- `GET /expense-tracking/summary/{}`
- `POST /expense-tracking/expenses`
- `PUT /expense-tracking/expenses/{}/approve`
- `PUT /expense-tracking/expenses/{}/reject`

## Functional design
Capture expense evidence, claimant, category, currency and approval status. Distinguish reimbursable claim, vendor bill and direct expense. Draft corrections allowed; posted corrections via reversal. Detect duplicate evidence without treating hash alone as proof of fraud.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Duplicate claim, rejected approval, partial reimbursement, tax treatment and attachment scope. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
