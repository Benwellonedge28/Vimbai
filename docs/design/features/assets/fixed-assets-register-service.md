# fixed-assets-register-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/assets`.

- `GET /fixed-assets-register/assets`
- `GET /fixed-assets-register/assets/{}`
- `GET /fixed-assets-register/assets/{}/depreciation`
- `GET /fixed-assets-register/summary`
- `POST /fixed-assets-register/assets`
- `POST /fixed-assets-register/assets/{}/depreciate`
- `POST /fixed-assets-register/assets/{}/dispose`

## Functional design
Maintain asset ID, acquisition evidence, cost, useful life, residual value, component and location history. Lifecycle draft, active, impaired, disposed; approved capitalization and depreciation events link to ledger. Do not store independently mutable carrying balance.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Duplicate acquisition, component replacement, depreciation, impairment, disposal and register-to-control reconciliation. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
