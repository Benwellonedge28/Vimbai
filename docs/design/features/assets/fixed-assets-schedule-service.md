# fixed-assets-schedule-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/assets`.

- `GET /fixed-assets-schedule/assets`
- `GET /fixed-assets-schedule/assets/{}`
- `GET /fixed-assets-schedule/schedule/latest`
- `GET /fixed-assets-schedule/summary`
- `POST /fixed-assets-schedule/assets`
- `POST /fixed-assets-schedule/schedule/generate`
- `PUT /fixed-assets-schedule/assets/{}`

## Functional design
Generate depreciation schedules from approved method, cost basis, residual value, useful life and in-service date. Changes in estimates apply according to policy and preserve prior periods. Schedule lines become posting intents once; distinguish projected from posted amounts.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Partial period, residual floor, estimate change, repeated run, disposal cutoff and exact schedule totals. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
