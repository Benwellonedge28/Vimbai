# rust

One workspace. Every crate Vimbai runs on, laid out so a crate's purpose is visible from its path.

```
apps/api-server                  the HTTP surface the gateway and clients call
crates/common                    Money, shared types
crates/kernel/ledger-core        posting integrity, immutability, the hash chain
crates/platform/identity         SSO
crates/platform/access-control/  RBAC and ABAC
crates/adapters/database         the store: Book-scoped reads and writes
crates/adapters/wire            every calculator JSON door, one shape per endpoint
crates/domains/automation        the automation engine
crates/calculations/             the seventeen calculation crates
```

Every crate is inside this one workspace and every crate forbids unsafe code. The PyO3 extension that
used to sit outside it is gone, so `cargo test --workspace` now builds everything that ships.

Crates are grouped by role, not by size. If a path is ambiguous, the crate belongs one level up
until it earns a directory of its own.
