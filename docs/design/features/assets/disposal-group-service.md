# disposal-group-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/assets`.

- `GET /disposal-group/groups`
- `GET /disposal-group/groups/{}`
- `GET /disposal-group/groups/{}/assets`
- `POST /disposal-group/groups`
- `POST /disposal-group/groups/{}/assets`
- `POST /disposal-group/groups/{}/impairment-test`
- `PUT /disposal-group/groups/{}/status`

## Functional design
Group assets and associated liabilities under approved classification policy. Record measurement basis, impairment allocation and disposal status; preserve member identities and prevent overlapping active groups. Changes require dated evidence.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Duplicate membership, impairment allocation, classification reversal, partial sale and group-to-register reconciliation. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
