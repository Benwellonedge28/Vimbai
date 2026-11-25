# cash-flow-service

Status: proposed feature specification; catalogue evidence at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Actual handler implementation not yet audited.

## Existing published surface
Owner: `rust/crates/domains/accounting`.

- `GET /cash-flow/get/{}`
- `GET /cash-flow/list`
- `POST /cash-flow/create`
- `POST /cash-flow/cumulative-flows`
- `POST /cash-flow/inflows`
- `POST /cash-flow/net-flows`
- `POST /cash-flow/outflows`

## Proposed design
Classify cash movements by operating, investing and financing policy; exclude noncash movements and internal cash transfers as appropriate. Reconcile opening and closing cash to ledger, separately identify FX effects. Do not maintain independent cash balances.

## Acceptance
Internal transfer, noncash transaction, FX effect, opening/closing reconciliation and period boundary.

## Integration contract
Reuse existing domain handlers and versioned OpenAPI; API server composes cross-domain calls. Authenticate principal, enforce Book scope, use exact Money, idempotent writes and existing error envelope. Flutter exposes only authorized capabilities. Persist via existing adapter; ledger-core is sole posting integrity authority. Inventory concrete source files and tests before implementation; no new parallel service or store.
