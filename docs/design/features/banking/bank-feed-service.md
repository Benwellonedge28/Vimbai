# bank-feed-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/banking`.

- `DELETE /bank-feed/connections/{}`
- `DELETE /bank-feed/rules/{}`
- `GET /bank-feed`
- `GET /bank-feed/connections`
- `GET /bank-feed/connections/{}`
- `GET /bank-feed/connections/{}/balance`
- `GET /bank-feed/rules`
- `GET /bank-feed/statistics`
- `GET /bank-feed/sync/history`
- `GET /bank-feed/sync/{}`
- `GET /bank-feed/transactions`
- `GET /bank-feed/transactions/{}`
- `POST /bank-feed/connections`
- `POST /bank-feed/rules`
- `POST /bank-feed/rules/apply`
- `POST /bank-feed/sync/start`
- `POST /bank-feed/transactions/import`
- `POST /bank-feed/transactions/import-batch`
- `POST /bank-feed/transactions/import-mt940`
- `POST /bank-feed/transactions/{}/link`
- `POST /bank-feed/transactions/{}/reconcile`
- `POST /bank-feed/webhooks/{}`
- `PUT /bank-feed/rules/{}`
- `PUT /bank-feed/transactions/{}/status`

## Functional design
Import immutable source transactions with provider/account/external ID, value date, currency, amount and provenance. Deduplicate by stable source identity; preserve raw evidence separately. Rule matching produces suggestions, not automatic posting unless approved policy permits. Reconciliation links existing ledger records; provider credentials remain server-side. MT940 parsing must reject malformed amounts and ambiguous currencies.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Duplicate imports, malformed MT940, interrupted sync, rule priority, unmatched transaction, revoked connection and reconciliation totals. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
