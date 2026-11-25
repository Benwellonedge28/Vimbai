# disposal-account-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/assets`.

- `GET /disposal-account/disposals`
- `GET /disposal-account/summary`
- `POST /disposal-account/disposal/calculate`
- `POST /disposal-account/disposal/post`

## Functional design
Calculate disposal proceeds, carrying amount and gain/loss from approved asset snapshot. Link disposal event to register and posting; partial disposal allocates carrying amount by approved basis. Distinguish tax and accounting treatment.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Full/partial disposal, zero proceeds, duplicate disposal, depreciation cutoff and ledger reconciliation. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
