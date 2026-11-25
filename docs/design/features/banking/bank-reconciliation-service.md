# bank-reconciliation-service

Status: proposed feature design. Catalogue baseline 2064d885c4082ed6632c567e19e305cbb3bfa21f; implementation behavior not independently verified.

## Existing published surface
Owner: `rust/crates/domains/banking`.

- `GET /bank-reconciliation/cash-book`
- `GET /bank-reconciliation/outstanding-items`
- `GET /bank-reconciliation/reconciliations`
- `GET /bank-reconciliation/reconciliations/{}`
- `GET /bank-reconciliation/statements`
- `GET /bank-reconciliation/statements/{}`
- `POST /bank-reconciliation/cash-book`
- `POST /bank-reconciliation/reconcile`
- `POST /bank-reconciliation/reconcile/{}/auto-match`
- `POST /bank-reconciliation/reconcile/{}/post-adjustments`
- `POST /bank-reconciliation/statements`

## Functional design
Create statement snapshot, cashbook projection and matching session at explicit cutoff. Support one-to-many matches, outstanding deposits and cheques, bank charges and approved adjustments. Lock completed reconciliation evidence; reopen through authorized revision. Never change bank statement source or plug a difference.

## Integration and integrity
Use existing handlers and OpenAPI contracts, authenticated Book scope, exact Money, durable idempotency and existing store adapter. Application composes cross-domain workflows; ledger-core validates posting integrity. Flutter presents authorized actions and pending/conflict states. Offline capture must preserve evidence and revalidate on replay. Inventory concrete source files, data schemas and tests before coding; avoid duplicate ownership.

## Acceptance criteria
Partial match, duplicate match, outstanding items, cutoff, adjustment reversal and zero unexplained difference. Also verify unauthorized access, invalid input, atomic failure and retry against actual handlers. Record test evidence at release revision.
