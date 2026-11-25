# provision-bad-debts-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/receivables`.

- `GET /provision-bad-debts/health`
- `GET /provision-bad-debts/policies`
- `GET /provision-bad-debts/provision-history`
- `POST /provision-bad-debts/calculate-provision`
- `POST /provision-bad-debts/debtors/add`
- `POST /provision-bad-debts/journal/create`
- `POST /provision-bad-debts/policies`

## Functional design
Version impairment methodology, eligible balances, calculation date and assumptions. Generate proposed adjustment against existing allowance, not a duplicate full provision; retain approval and calculation evidence.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Zero exposure, changed allowance, repeated calculation, reversal and control-account reconciliation. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
