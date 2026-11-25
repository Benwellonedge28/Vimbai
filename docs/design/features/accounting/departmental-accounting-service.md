# departmental-accounting-service

Status: proposed feature specification; catalogue evidence at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Actual handler implementation not yet audited.

## Existing published surface
Owner: `rust/crates/domains/accounting`.

- `DELETE /departmental-accounting/allocation-rules/{}`
- `GET /departmental-accounting/allocation-rules`
- `GET /departmental-accounting/allocations/{}`
- `GET /departmental-accounting/cost-pools`
- `GET /departmental-accounting/cost-pools/{}`
- `GET /departmental-accounting/departments`
- `GET /departmental-accounting/departments/{}`
- `GET /departmental-accounting/departments/{}/financials`
- `GET /departmental-accounting/departments/{}/hierarchy`
- `GET /departmental-accounting/departments/{}/performance-report`
- `GET /departmental-accounting/inter-department-bills`
- `GET /departmental-accounting/reports/cost-distribution`
- `GET /departmental-accounting/reports/department-comparison`
- `POST /departmental-accounting/allocate`
- `POST /departmental-accounting/allocation-rules`
- `POST /departmental-accounting/cost-pools`
- `POST /departmental-accounting/departments`
- `POST /departmental-accounting/inter-department-bills`
- `POST /departmental-accounting/inter-department-bills/{}/approve`
- `POST /departmental-accounting/reports/department-comparison`
- `PUT /departmental-accounting/departments/{}`

## Proposed design
Department and cost centre identifiers remain stable; creation is atomic where required. Allocate income/expenses through approved dimensions and canonical costing rules. Department reports reconcile to Book totals; shared costs retain allocation evidence.

## Acceptance
Duplicate department, invalid dimension, allocation residual, department closure and aggregate reconciliation.

## Integration contract
Reuse existing domain handlers and versioned OpenAPI; API server composes cross-domain calls. Authenticate principal, enforce Book scope, use exact Money, idempotent writes and existing error envelope. Flutter exposes only authorized capabilities. Persist via existing adapter; ledger-core is sole posting integrity authority. Inventory concrete source files and tests before implementation; no new parallel service or store.
