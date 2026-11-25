# Master architecture
Status: proposed; baseline a9ee8a08e9d46c08f84292749244fe284e948150.

## Authority and topology
Extend [system boundaries](../architecture/system-boundaries.md), not replace them. Flutter owns presentation; Go gateway owns ingress authentication, routing and resilience; Rust API server delegates to domains; ledger-core alone validates and commits posting integrity; ledger-services reads and reconciles. Local runtime uses the same engine; sync-server exchanges Book records; worker executes durable jobs. No new service per feature.

## Contracts
Every command carries authenticated principal, Book scope, request/idempotency identifier and expected revision where applicable. Derive scope server-side. Return typed validation, authorization, conflict, unavailable and integrity errors through ADR-004. Cross-domain orchestration belongs in application/API composition, not domain-to-domain dependencies. Exact Money is canonical; no client recomputation of authoritative totals.

## Integration plan
Extend rust/crates/platform for shared policies, documents and events; adapters/database for persistence; existing domains for business rules; rust/apps/api-server for composition; apps/flutter/lib/features for presentation. Register new routes in existing catalogue and OpenAPI, regenerate route snapshot. Preserve ADR-001–015; conflicting changes require explicit ADR amendments.

## Acceptance
Dependency and safety gates, contract compatibility, tenant isolation, exact-money round trips, local/remote parity, replay and crash recovery. No duplicate ledger, identity, storage or calculation authority. Existing documentation is baseline evidence, not proof every route is production-ready.
