# Database migration
Status: proposed extension of ADR-007 and ADR-011. Reuse vimbai-store-move rather than create a second migration engine.

## Protocol
Discover source/target capabilities and schema versions; acquire migration lease; take verified backup and consistent snapshot; stage target; copy records and edges preserving identity and exact values; verify counts, canonical digests, ledger chains, balances and relationships; replay captured changes if supported or keep writes quiesced; atomically switch deployment configuration; retain source read-only until rollback window closes.

## Safety
Never infer relationships from current business rules. Never report success on partial copy. Persist checkpoint and manifest with source identity, snapshot revision, schema versions and verification results. Retry idempotently. Fail closed on unsupported type, scope mismatch, duplicate ID or integrity discrepancy. Rollback after new target writes requires reverse replication or reconciliation, not blind pointer reversal.

## Integration
Existing store-move tool owns transport; database adapter owns schema transformations; operations layer owns lease, backup, cutover and audit. Automatic planning and verification are desirable; production cutover requires authorized approval.

## Acceptance
Round-trip SQLite/PostgreSQL, interrupted copy, concurrent write fencing, corruption detection, rollback rehearsal and exact ledger equivalence. Measure downtime and recovery against deployment-specific objectives.
