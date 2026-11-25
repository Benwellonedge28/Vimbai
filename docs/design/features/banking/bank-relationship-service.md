# bank-relationship-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/banking`.

- `GET /bank-relationship/quality-metrics/{}`
- `GET /bank-relationship/relationships/{}`
- `GET /bank-relationship/summary/{}`
- `POST /bank-relationship/quality-metrics`
- `POST /bank-relationship/relationships`
- `PUT /bank-relationship/relationships/{}`

## Functional design
Maintain institution relationship, authorized contacts, account references and effective-dated service quality observations. Define metric units and observation windows; distinguish subjective ratings from measured values. Restrict sensitive contacts and avoid storing banking credentials.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Duplicate institution, expired relationship, missing metric, unauthorized contact access and historical summary. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
