# accounting-standards-service

Status: proposed feature specification; catalogue evidence at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Actual handler implementation not yet audited.

## Existing published surface
Owner: `rust/crates/domains/accounting`.

- `GET /accounting-standards/organizations/{}/compliance/history`
- `GET /accounting-standards/organizations/{}/configuration`
- `GET /accounting-standards/organizations/{}/policies`
- `GET /accounting-standards/organizations/{}/policies/{}`
- `GET /accounting-standards/standards`
- `GET /accounting-standards/standards/categories`
- `GET /accounting-standards/standards/comparison`
- `GET /accounting-standards/standards/{}`
- `GET /accounting-standards/standards/{}/account-mapping`
- `GET /accounting-standards/standards/{}/disclosure-requirements`
- `GET /accounting-standards/standards/{}/measurement-guide`
- `GET /accounting-standards/standards/{}/requirements`
- `POST /accounting-standards/organizations/{}/compliance/check`
- `POST /accounting-standards/organizations/{}/configuration`
- `POST /accounting-standards/organizations/{}/policies`
- `POST /accounting-standards/organizations/{}/standards/{}/activate`
- `POST /accounting-standards/organizations/{}/standards/{}/deactivate`
- `POST /accounting-standards/standards/{}/account-mapping`
- `PUT /accounting-standards/organizations/{}/configuration`

## Proposed design
Version standards, requirements, account mappings, measurement guidance and disclosure rules. Organisation activation requires authorized approval, effective dates and eligibility checks. Compliance history records policy version, evidence and exceptions; it is not a certification. Preserve historical reports after policy changes.

## Acceptance
Conflicting activation, missing mappings, effective-date boundaries, historical reproduction and expert-reviewed fixtures.

## Integration contract
Reuse existing domain handlers and versioned OpenAPI; API server composes cross-domain calls. Authenticate principal, enforce Book scope, use exact Money, idempotent writes and existing error envelope. Flutter exposes only authorized capabilities. Persist via existing adapter; ledger-core is sole posting integrity authority. Inventory concrete source files and tests before implementation; no new parallel service or store.
