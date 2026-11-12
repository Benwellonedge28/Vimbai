# rust

One workspace. Every crate Vimbai runs on, laid out so a crate's purpose is visible from its path.

```
apps/api-server                  the HTTP surface the gateway and clients call
crates/common                    Money, shared types
crates/kernel/ledger-core        posting integrity, immutability, the hash chain
crates/platform/identity         SSO
crates/platform/access-control/  RBAC and ABAC
crates/adapters/database         the store: Book-scoped reads and writes
crates/adapters/python-bridge    transitional PyO3 seam, to be removed
crates/domains/automation        the automation engine
crates/calculations/             the seventeen calculation crates
native/                          the PyO3 extension, deliberately outside the workspace
```

`native/` is excluded because it links libpython: `cargo test --workspace` never builds what Python
actually loads, which is why `scripts/preflight.sh` checks it separately.

Crates are grouped by role, not by size. If a path is ambiguous, the crate belongs one level up
until it earns a directory of its own.
