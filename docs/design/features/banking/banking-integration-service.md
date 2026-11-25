# banking-integration-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/banking`.

- `DELETE /banking-integration/connections/{}`
- `GET /banking-integration/connections`
- `GET /banking-integration/health`
- `GET /banking-integration/transactions/{}`
- `POST /banking-integration/connect`
- `POST /banking-integration/sync/{}`
- `POST /banking-integration/transactions/{}/reconcile`

## Functional design
Define read-only connection capability, consent scope, expiry and sync cursor. External adapter handles authentication and retrieval; bank-feed owns normalized imports and reconciliation owner links records. No transfers or payment initiation. Retry with bounded backoff and explicit stale-data status.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Expired consent, revoked connection, timeout, pagination, duplicate import and unsupported provider capability. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
