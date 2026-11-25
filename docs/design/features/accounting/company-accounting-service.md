# company-accounting-service

Status: proposed feature specification; catalogue evidence at 2064d885c4082ed6632c567e19e305cbb3bfa21f. Actual handler implementation not yet audited.

## Existing published surface
Owner: `rust/crates/domains/accounting`.

- `GET /company-accounting/capital-transactions`
- `GET /company-accounting/companies`
- `GET /company-accounting/companies/{}`
- `GET /company-accounting/dividends`
- `GET /company-accounting/dividends/{}/payments`
- `GET /company-accounting/reports/dividend-history/{}`
- `GET /company-accounting/reports/equity-statement/{}`
- `GET /company-accounting/reports/shareholder-register/{}`
- `GET /company-accounting/reserves`
- `GET /company-accounting/retained-earnings`
- `GET /company-accounting/share-capital`
- `GET /company-accounting/shareholders`
- `POST /company-accounting/capital-transactions`
- `POST /company-accounting/companies`
- `POST /company-accounting/dividends`
- `POST /company-accounting/dividends/{}/pay`
- `POST /company-accounting/reserves`
- `POST /company-accounting/retained-earnings`
- `POST /company-accounting/share-capital`
- `POST /company-accounting/shareholders`
- `PUT /company-accounting/companies/{}`
- `PUT /company-accounting/dividends/{}/pay`

## Proposed design
Company profile configures reporting and governance. Delegate share capital, reserves and distributions to equity; accounting owns statement assembly and period orchestration. Preserve existing routes through composition; no duplicate capital calculations.

## Acceptance
Capital issue, dividend, reserve transfer, closing and equity-to-ledger reconciliation.

## Integration contract
Reuse existing domain handlers and versioned OpenAPI; API server composes cross-domain calls. Authenticate principal, enforce Book scope, use exact Money, idempotent writes and existing error envelope. Flutter exposes only authorized capabilities. Persist via existing adapter; ledger-core is sole posting integrity authority. Inventory concrete source files and tests before implementation; no new parallel service or store.
