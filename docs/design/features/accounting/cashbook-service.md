# cashbook-service

Status: proposed feature specification; catalogue evidence at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Actual handler implementation not yet audited.

## Existing published surface
Owner: `rust/crates/domains/accounting`.

- `GET /cashbook/accounts`
- `GET /cashbook/accounts/{}`
- `GET /cashbook/cash-flow`
- `GET /cashbook/cash-flow/summary`
- `GET /cashbook/cash-position`
- `GET /cashbook/cash-position/history`
- `GET /cashbook/entries`
- `GET /cashbook/entries/{}`
- `GET /cashbook/reconciliations`
- `GET /cashbook/reconciliations/{}`
- `GET /cashbook/reconciliations/{}/outstanding`
- `GET /cashbook/summary/{}`
- `POST /cashbook/accounts`
- `POST /cashbook/cash-flow`
- `POST /cashbook/entries`
- `POST /cashbook/reconciliations`
- `PUT /cashbook/accounts/{}`
- `PUT /cashbook/entries/{}/post`
- `PUT /cashbook/entries/{}/void`
- `PUT /cashbook/reconciliations/{}/complete`

## Proposed design
Record receipts and disbursements as source-linked posting intents; distinguish bank/cash accounts and reconciliation status. Cashbook is a projection, not a second ledger. Bank reconciliation belongs to banking; liquidity forecasting to treasury. Corrections reverse posted entries.

## Acceptance
Duplicate receipt, transfer, outstanding cheque, reversal, bank reconciliation and scope denial.

## Integration contract
Reuse existing domain handlers and versioned OpenAPI; API server composes cross-domain calls. Authenticate principal, enforce Book scope, use exact Money, idempotent writes and existing error envelope. Flutter exposes only authorized capabilities. Persist via existing adapter; ledger-core is sole posting integrity authority. Inventory concrete source files and tests before implementation; no new parallel service or store.
