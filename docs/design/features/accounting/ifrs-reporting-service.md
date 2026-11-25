# ifrs-reporting-service

Status: proposed feature specification; catalogue evidence at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Actual handler implementation not yet audited.

## Existing published surface
Owner: `rust/crates/domains/accounting`.

- `GET /ifrs-reporting/reports`
- `GET /ifrs-reporting/reports/{}`
- `GET /ifrs-reporting/reports/{}/notes`
- `POST /ifrs-reporting/reports`
- `POST /ifrs-reporting/reports/{}/notes`
- `PUT /ifrs-reporting/reports/{}/approve`

## Proposed design
Generate statements and disclosures from versioned IFRS policy packs and ledger snapshots. Identify missing disclosures and unsupported treatments; never assert compliance automatically. Preserve comparatives and approved adjustments.

## Acceptance
Disclosure completeness, comparative consistency, missing policy and expert-reviewed golden reports.

## Integration contract
Reuse existing domain handlers and versioned OpenAPI; API server composes cross-domain calls. Authenticate principal, enforce Book scope, use exact Money, idempotent writes and existing error envelope. Flutter exposes only authorized capabilities. Persist via existing adapter; ledger-core is sole posting integrity authority. Inventory concrete source files and tests before implementation; no new parallel service or store.
