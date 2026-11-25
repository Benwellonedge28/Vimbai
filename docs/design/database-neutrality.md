# Database neutrality
Status: proposed extension of ADR-007. Existing Graph trait, PostgreSQL deployment, SQLite device and retained Neo4j backend are the starting point.

## Provider contract
Use semantic record/edge operations, scoped queries, atomic transactions, uniqueness, revision checks, consistent snapshots and durable recovery. Do not expose SQL/Cypher to domains. Provider registry advertises verified capabilities, schema versions and deployment compatibility. Select explicitly configured provider or eligible profile default; never silently switch a live authoritative store after an error. Unsupported capability fails before writes.

## Integration
Extend rust/crates/adapters/database and existing vimbai_store::Graph seam; inventory actual adapter files before adding modules. Keep Money aggregation canonical in Rust. Preserve IDs, Book scope, edges, exact decimal representation, sequence and idempotency across providers. Neo4j is not required for new features; use only existing supported role or separately approved read model.

## Acceptance
Shared conformance suite against real PostgreSQL and SQLite plus retained backend where supported. Verify transaction rollback, concurrent posting, exact decimal round trips, tenant isolation, snapshot consistency and failure injection. Provider selection is configuration, not business logic. No claim that arbitrary databases satisfy ledger integrity.
