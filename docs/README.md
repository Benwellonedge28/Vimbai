# Vimbai documentation

Where to read what. The repository is the source of truth; this index exists so nobody has to guess which of ten
documents is current.

## Start here

- `ROADMAP.md` - what is being built, in what order, and what proves each step is finished. The working plan.
- `Vimbai_Design_Document_V1.5.md` - the product and functional design the platform is built to.

## Decisions (binding)

`decisions/` holds ADRs. These are normative: code that contradicts one is a defect, or the ADR is amended in the same
pull request.

| ADR | Decision |
| --- | --- |
| 001 | Language policy: Safe Rust backend, Go gateway, Flutter client, no Python |
| 002 | `kernel/ledger-core` is the sole posting authority; domains orchestrate |
| 003 | One crate per business domain, and the five recorded divergences from the proposed tree |
| 004 | API versioning and the conventions clients can rely on |
| 005 | Vimbai Pay is not built |
| 006 | The developer platform is deferred, with its trigger conditions |
| 007 | One system of record (Postgres) with SQLite on a device, and the tool that moves the data off it |
| 008 | Crate directory names are not renamed for cosmetic consistency |
| 009 | The kernel's reading half reads the store; the posting authority stays store-free |

## Architecture

- `architecture/overview.md` - **start here.** The platform as built: one engine, what each binary may and may not do,
  the crate families, how money is carried, where the tree differs from the design document and why.
- `architecture/system-boundaries.md` - what each layer owns, and where one part stops and the next begins.
- `architecture/dependency-rules.md` - the rules between crates, the one tolerated dependency, and the gate behind them.
  `cargo run -p xtask -- boundaries` applies them; a test fails if this page and the code disagree.
- `architecture/RUST-CRATE-LAYOUT.md` - historical snapshot of the crate layout. The current boundaries are ADR-003.

## Operations

- `LOCAL-RUNTIME.md` - **clone to running.** The on-device runtime: one binary, one data directory, the engine and the
  gateway's rules in one process. Start here if you want something running before reading anything else.
- `WORKER.md` - the scheduled-automation worker: the schedule a rule carries, and how a run happens exactly once.
- `DEVELOPER_GUIDE.md` - working on the codebase day to day.
- `routes-snapshot.json` - the published address set, useful for diffing after a change.

## Domains

`domains/` documents each business domain as built: what it owns, the services and gateway prefixes it answers on, and
what it deliberately does not own. One page per domain, twenty-two pages, and the mapping is a gate rather than a
convention: `cargo run -p xtask -- domains` fails if a routed service has no page, if a page names a service or crate the
catalogue does not have, or if a page leaves out one of its own prefixes.

`domains/payments.md` states POS till support only, and that Vimbai Pay is not built, per ADR-005.

## Migration

- `migration/migration-status.md` - **complete.** What moved, how each retiring service was held to its published
  addresses, and the five outcomes that are decisions rather than leftovers.
- `migration/MIGRATION-MAP.md`, `MIGRATION-MANIFEST.md`, `RUST-MIGRATION-BACKLOG.md` - snapshots from while it was
  running. Kept as history, not maintained. The live inventory is `catalog/services/catalog.yaml`; the live gate is
  `cargo run -p xtask -- gateway`.

## Not written yet, and why

A Postgres implementation of the store (ADR-007 decides it; the driver and a server to test against are not in this
repository yet). Attaching the 180 unattributed prefix groups to catalogue owners. Operations runbooks beyond the
manifests. Compliance detail beyond financial integrity. Anything belonging to Vimbai Pay or the developer platform,
which are out of scope by ADR-005 and ADR-006.
