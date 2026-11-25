# Self-hosting and independent deployments
Status: proposed extension of LOCAL-RUNTIME.md, ADR-007 and operations runbook.

## Profiles
Single-device local runtime; single-server deployment; institutional private deployment; managed cloud. Same engine and ledger rules. PostgreSQL deployment and SQLite device are ADR-007 targets; LOCAL-RUNTIME.md describes file storage, so reconcile actual runtime default before claiming migration complete.

## Deployment contract
Versioned configuration for database, network, identity, storage, keys, backups, observability and enabled capabilities. Secrets outside source. TLS at ingress, least privilege, isolated service accounts, health/readiness checks and explicit migration command. No mandatory external telemetry or cloud dependency for offline profile.

## Operations
Provide reproducible packages and deployment manifests, backup/restore procedures, upgrade compatibility matrix, capacity planning and disaster recovery. Customer controls data location and keys where deployment permits. Licensing/entitlement offline verification must not prevent lawful data export.

## Acceptance
Fresh install without developer tools, restore on replacement host, upgrade/rollback rehearsal, network isolation, certificate rotation, tenant isolation and local/remote parity. Independent deployment support requires documented operational ownership and tested recovery objectives.
