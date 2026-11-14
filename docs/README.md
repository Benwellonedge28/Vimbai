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
| 008 | Crate directory names are not renamed for cosmetic consistency |

ADR-007 (data storage) is deliberately absent. It is written when the storage direction is decided; see the blocked phase
in `ROADMAP.md`.

## Architecture

- `architecture/system-boundaries.md` - what each layer owns. Roadmap P1.
- `architecture/dependency-rules.md` - the rules between crates, and the gate that enforces them. Roadmap P1.
- `architecture/RUST-CRATE-LAYOUT.md` - historical snapshot of the crate layout. The current boundaries are ADR-003.

## Domains

`domains/` documents each business domain as built: what it owns, the addresses it serves, and what it deliberately does
not own. Written in roadmap P5. `domains/payments.md` states POS till support only, per ADR-005.

## Operations and local use

- `LOCAL-RUNTIME.md` - the on-device runtime: one binary, one data directory, offline behaviour.
- `DEVELOPER_GUIDE.md` - working on the codebase day to day.
- `routes-snapshot.json` - the published address set, useful for diffing after a change.

## Migration (historical)

`migration/` keeps the documents that recorded the Python-to-Rust move. They are snapshots, annotated as such, and are not
maintained. The live inventory is `catalog/services/catalog.yaml`; the live gate is `cargo run -p xtask -- gateway`.

## Not written yet, and why

Contract policies (roadmap P6), data ownership and migration (blocked on the storage decision), operations runbooks and
deployment topology (roadmap P10), compliance detail beyond financial integrity (roadmap P3), and anything belonging to
Vimbai Pay or the developer platform, which are out of scope by ADR-005 and ADR-006.
