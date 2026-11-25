# intangible-assets-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/assets`.

- `GET /intangible-assets/assets`
- `GET /intangible-assets/assets/{}`
- `GET /intangible-assets/assets/{}/amortization`
- `POST /intangible-assets/assets`
- `POST /intangible-assets/assets/{}/amortize`

## Functional design
Track identifiable intangible assets, recognition evidence, finite/indefinite life and amortization or impairment policy. Distinguish research expense from qualifying development cost under approved standard. Preserve valuation and review history.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Unsupported capitalization, finite-life amortization, indefinite-life review, impairment and disposal. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
