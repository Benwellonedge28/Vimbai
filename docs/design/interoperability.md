# Integration and interoperability
Status: proposed extension of ADR-004, ADR-006 and platform integration contracts.

## Contracts
Preserve published unversioned aliases and versioned OpenAPI routes. Additive changes preferred; breaking changes require version/deprecation policy. Money is exact decimal string, binary base64, identifiers opaque, timestamps ISO 8601. Use existing error envelope and cursor conventions. Idempotency keys scoped to caller/operation; conflicting payload reuse refused.

## Integration boundaries
Platform integrations and webhooks own external protocol plumbing; domains own business validation; application composes use cases; gateway authenticates and limits ingress. Import adapters normalize external records into validated drafts, never directly write posted ledger lines. Export includes IDs, relationships, currency, policy version and provenance. Signed webhooks use durable outbox, retry/backoff and deduplication. No automatic API credential product until ADR-006 trigger is met.

## Formats and lifecycle
Support versioned CSV/JSON and applicable accounting/banking formats through explicit mappings; validate schema, preview errors and reconcile totals. Maintain compatibility fixtures and provider capability declarations. External failures cannot corrupt local ledger state.

## Acceptance
Round-trip export/import, malformed records, duplicate delivery, out-of-order events, revoked credentials, pagination, version compatibility and cross-tenant denial. Contract gate verifies route/spec parity. No claim of support for an external standard until tested.
