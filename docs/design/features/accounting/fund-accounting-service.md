# fund-accounting-service

Status: proposed feature specification; catalogue evidence at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Actual handler implementation not yet audited.

## Existing published surface
Owner: `rust/crates/domains/accounting`.

- `GET /fund-accounting/funds/{}`
- `GET /fund-accounting/transactions/{}`
- `POST /fund-accounting/funds`
- `POST /fund-accounting/transactions`

## Proposed design
Fund restrictions and donor obligations belong to NPO/fund policy; accounting composes posting and reports. Track restricted/unrestricted balances as dimensions with approved transfers; no separate ledger. Public-sector appropriations require separate budget control.

## Acceptance
Restricted expenditure refusal, authorized transfer, grant release and fund-to-ledger reconciliation.

## Integration contract
Reuse existing domain handlers and versioned OpenAPI; API server composes cross-domain calls. Authenticate principal, enforce Book scope, use exact Money, idempotent writes and existing error envelope. Flutter exposes only authorized capabilities. Persist via existing adapter; ledger-core is sole posting integrity authority. Inventory concrete source files and tests before implementation; no new parallel service or store.
