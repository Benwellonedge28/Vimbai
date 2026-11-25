# investment-property-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/assets`.

- `POST /investment-property/cost-model`
- `POST /investment-property/fair-value-disclosure`
- `POST /investment-property/fair-value-model`
- `POST /investment-property/transfer`
- `POST /investment-property/yields`

## Functional design
Record property classification, use, measurement model, valuation date and evidence. Apply approved cost or fair-value model; transfers require documented change of use and effective date. Route adjustments through ledger authority.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Model consistency, transfer, missing valuation, disposal and carrying-value reconciliation. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
