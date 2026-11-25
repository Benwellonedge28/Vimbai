# currency-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/banking`.

- `DELETE /currency/currencies/{}`
- `GET /currency/currencies`
- `GET /currency/currencies/{}`
- `GET /currency/format/{}/{}`
- `GET /currency/rates`
- `GET /currency/rates/history/{}/{}`
- `GET /currency/rates/latest`
- `GET /currency/rates/{}/{}`
- `GET /currency/stats`
- `POST /currency/alerts/rate`
- `POST /currency/convert`
- `POST /currency/convert/batch`
- `POST /currency/currencies`
- `POST /currency/rates`
- `POST /currency/triangulate`
- `POST /currency/validate-transaction`
- `PUT /currency/currencies/{}`
- `PUT /currency/rates/{}/{}`

## Functional design
Maintain currency identity, precision metadata and effective-dated rate observations with source and quote convention. Reuse common FX arithmetic; distinguish display precision from ledger precision. No silent conversion when rate missing; retain original currency and rate on financial records.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Inverse rate, invalid rate, historical currency, stale quote, rounding and exact conversion. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
