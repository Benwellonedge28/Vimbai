# balance-sheet-service

Status: proposed feature specification; catalogue evidence at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Actual handler implementation not yet audited.

## Existing published surface
Owner: `rust/crates/domains/accounting`.

- `GET /balance-sheet/history/{}`
- `GET /balance-sheet/latest/{}`
- `GET /balance-sheet/ratios/{}`
- `POST /balance-sheet/generate`

## Proposed design
Generate as-of statements from posted ledger snapshot, classified by approved chart mapping. Assets equal liabilities plus equity; disclose exceptions rather than plug differences. History retains snapshot and policy version; ratios use canonical calculations and explicit undefined denominator handling.

## Acceptance
Trial balance reconciliation, comparative periods, negative balances, missing classifications, zero denominator and immutable report history.

## Integration contract
Reuse existing domain handlers and versioned OpenAPI; API server composes cross-domain calls. Authenticate principal, enforce Book scope, use exact Money, idempotent writes and existing error envelope. Flutter exposes only authorized capabilities. Persist via existing adapter; ledger-core is sole posting integrity authority. Inventory concrete source files and tests before implementation; no new parallel service or store.
