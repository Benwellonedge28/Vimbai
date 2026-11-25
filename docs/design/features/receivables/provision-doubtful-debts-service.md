# provision-doubtful-debts-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/receivables`.

- `GET /provision-doubtful-debts/debtors`
- `GET /provision-doubtful-debts/provisions`
- `POST /provision-doubtful-debts/calculate-provision`
- `POST /provision-doubtful-debts/debtors/add`
- `POST /provision-doubtful-debts/debtors/{}/assess`
- `POST /provision-doubtful-debts/provision/create`

## Functional design
Age receivables at explicit date; apply approved risk categories and rates using exact amounts. Coordinate with bad-debt provision owner to prevent overlapping allowance. Separate estimation from write-off.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Age boundary, disputed invoice, rate change, duplicate allowance and historical reproducibility. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
