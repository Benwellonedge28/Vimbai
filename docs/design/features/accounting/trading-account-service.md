# trading-account-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/accounting`.

- `GET /trading-account/accounts`
- `GET /trading-account/accounts/{}`
- `POST /trading-account/generate`

## Functional design
Derive net sales, cost of goods sold and gross profit from consistent posted snapshot and inventory policy. Separate returns, discounts and opening/closing stock; delegate valuation to canonical costing rules. Never plug missing stock figures.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Returns, zero sales, missing inventory policy, period cutoffs and reconciliation to income statement. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
