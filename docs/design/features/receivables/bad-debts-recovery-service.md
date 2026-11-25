# bad-debts-recovery-service

Status: proposed feature design; published catalogue at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Handler behavior not yet independently verified.

## Published API and ownership
Owner: `rust/crates/domains/receivables`.

- `GET /bad-debts-recovery/bad-debts`
- `GET /bad-debts-recovery/bad-debts/{}`
- `GET /bad-debts-recovery/health`
- `GET /bad-debts-recovery/recovery-summary`
- `POST /bad-debts-recovery/bad-debts/register`
- `POST /bad-debts-recovery/bad-debts/{}/recover`

## Functional design
Link recovery to original written-off obligation, record receipt evidence and approved recovery posting. Cap recovery against outstanding recoverable amount unless separately classified; preserve original write-off.

## Integration and data integrity
Preserve existing route contracts and exact Money. Resolve authenticated principal and Book scope before reads or writes. Domain owns business validation; application composes cross-domain operations; ledger-core owns posting integrity. Use existing persistence adapter and idempotency mechanism. Flutter displays authorized actions and explicit pending/conflict states; offline operations require validated replay. Inventory actual handler, model, store and test files before coding; do not create duplicate authority.

## Acceptance criteria
Partial recovery, duplicate receipt, over-recovery, foreign currency and original debt trace. Verify scope denial, invalid input, retry and atomic failure against real handlers. Record source revision and test evidence before declaring implementation complete.
