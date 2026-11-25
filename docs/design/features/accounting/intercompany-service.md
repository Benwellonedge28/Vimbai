# intercompany-service

Status: proposed feature specification; catalogue evidence at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Actual handler implementation not yet audited.

## Existing published surface
Owner: `rust/crates/domains/accounting`.

- `GET /intercompany/eliminations`
- `GET /intercompany/entities`
- `GET /intercompany/transactions`
- `POST /intercompany/entities`
- `POST /intercompany/transactions`
- `POST /intercompany/transactions/match`

## Proposed design
Link reciprocal transactions across authorized Books without cross-tenant access leakage. Match balances by counterparty, currency and period; consolidation eliminations occur in reporting/consolidation scope, never erase source entries. Handle timing and FX differences explicitly.

## Acceptance
Unmatched pair, duplicate elimination, unauthorized Book, FX mismatch and consolidated reconciliation.

## Integration contract
Reuse existing domain handlers and versioned OpenAPI; API server composes cross-domain calls. Authenticate principal, enforce Book scope, use exact Money, idempotent writes and existing error envelope. Flutter exposes only authorized capabilities. Persist via existing adapter; ledger-core is sole posting integrity authority. Inventory concrete source files and tests before implementation; no new parallel service or store.
