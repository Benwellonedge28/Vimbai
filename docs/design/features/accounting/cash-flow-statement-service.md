# cash-flow-statement-service

Status: proposed feature specification; catalogue evidence at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Actual handler implementation not yet audited.

## Existing published surface
Owner: `rust/crates/domains/accounting`.

- `GET /cash-flow-statement/history/{}`
- `GET /cash-flow-statement/latest/{}`
- `POST /cash-flow-statement/generate`

## Proposed design
Assemble direct or indirect statement using approved policy and canonical cash flow facts. Indirect method reconciles profit, noncash adjustments and working capital movements. Preserve source references, comparative periods and report snapshot.

## Acceptance
Direct/indirect reconciliation, depreciation adjustment, working capital changes, noncash acquisition and exact totals.

## Integration contract
Reuse existing domain handlers and versioned OpenAPI; API server composes cross-domain calls. Authenticate principal, enforce Book scope, use exact Money, idempotent writes and existing error envelope. Flutter exposes only authorized capabilities. Persist via existing adapter; ledger-core is sole posting integrity authority. Inventory concrete source files and tests before implementation; no new parallel service or store.
