# biological-assets-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/assets`.

- `POST /biological-assets/consumable-bearer-distinction`
- `POST /biological-assets/disclosure-template`
- `POST /biological-assets/fair-value-level1`
- `POST /biological-assets/fair-value-level2`
- `POST /biological-assets/fair-value-level3`
- `POST /biological-assets/measure`

## Functional design
Represent asset class, bearer/consumable distinction, measurement date, quantity, valuation inputs and evidence. Apply approved jurisdiction/standard policy; fair-value hierarchy identifies observable inputs and uncertainty. Generate proposed measurement/disclosure, not automatic compliance claim.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Missing valuation evidence, classification, hierarchy level, quantity change and approved adjustment reconciliation. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
