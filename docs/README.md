# Vimbai documentation

Where to read what. The repository is the source of truth; this index exists so nobody has to guess which
document is current. It is checked rather than trusted: `cargo run -p xtask -- docs` fails if a document under `docs/` is
missing from here, if a path named here does not exist, if a command named here is not a command `xtask` has, or if the
domain-page count beside it disagrees with `domains/`. Across the whole of `docs/`, a decision cited by its file name and a
workflow it names by file also have to exist, because a dead reference is a reader's next click. `cargo run -p xtask -- safety` (or `make safety`) holds the two standing rules that the deleted Python suite used to check:
every crate root forbids unsafe code - integration test crates included, because each one is its own crate - no real `unsafe`
block anywhere, and no Python in the repository.

A page that names the test proving one of its claims is checked against the file it names: that test has to be defined there.

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
| 010 | An organisation is the tenant, a Book belongs to one, and one rule decides who may open it |
| 011 | A schema change is a numbered step, recorded, in both stores |
| 012 | A Book is kept in one money; exchange differences are reported, never absorbed |
| 013 | A Book is autonomous: an entry flows to the reports, and a report is a view over any window |
| 014 | Nothing business-specific is code - a Book's measures and labels are its own data |
| 015 | A ledger is created when a posting needs it, and control accounts are never invented |
| 016 | A rule's step reaches the engine, and a Book opens with the clock's work already done |
| 017 | A measure is a row in the Book, built from six sources and checked against the Book's own chart |

## Architecture

- `architecture/overview.md` - **start here.** The platform as built: one engine, what each binary may and may not do,
  the crate families, how money is carried, where the tree differs from the design document and why.
- `architecture/system-boundaries.md` - what each layer owns, and where one part stops and the next begins.
- `architecture/dependency-rules.md` - the rules between crates, the one tolerated dependency, and the gate behind them.
  `cargo run -p xtask -- boundaries` applies them; a test fails if this page and the code disagree.
- `architecture/DUPLICATION.md` - historical audit of the duplicated maths in the Python estate, kept because it explains
  why several calculators have one home today. Not maintained.
- `architecture/RUST-CRATE-LAYOUT.md` - historical snapshot of the crate layout. The current boundaries are ADR-003.
- `flutter-structure.md` - the app's feature-first layout, and the gate that keeps files from drifting back into layer
  folders.

## Proposed design work packages

- `architecture/design-reading-order.md` - dependency order, review decisions, and links to each package.
- `architecture/book-policy-gate.md` - Enforce the boundary between Book policy and engine logic.
- `architecture/posting-account-resolution.md` - Resolve the account a posting needs.
- `architecture/book-access-and-navigation.md` - Open the right Book and show actions the caller can use.
- `domains/financial-report-completeness.md` - Produce complete reconciliation and ageing reports.
- `architecture/portable-book-recovery.md` - Restore a Book with unsynced work after device failure.
- `architecture/android-pilot-release.md` - Build, distribute and verify the Android pilot.
- `architecture/commercial-subscriptions.md` - Grant one Vimbai subscription across Android, Windows and web.
- `architecture/referral-awards.md` - Record referral awards and partner commissions.
- `architecture/design-continuation-order.md` - reading order and dependencies for packages 09–14.
- `architecture/windows-offline-release.md` - Ship an offline Windows build with the local accounting engine.
- `domains/accounting-policy-evidence.md` - Version accounting standards and preserve evidence for each report.
- `domains/jurisdiction-tax-rule-versions.md` - Use explicitly approved tax rules for each jurisdiction and effective date.
- `domains/statement-import-and-replay.md` - Import a bank statement with a reviewable receipt.
- `domains/government-budget-control-pilot.md` - Approve and account for one department’s budget commitments.
- `architecture/partner-event-delivery.md` - Deliver one partner event safely after the integration trigger.

## Operations

- `LOCAL-RUNTIME.md` - **clone to running.** The on-device runtime: one binary, one data directory, the engine and the
  gateway's rules in one process. Start here if you want something running before reading anything else.
- `WORKER.md` - the scheduled-automation worker: the schedule a rule carries, and how a run happens exactly once.
- `operations/RUNBOOK.md` - **when something is broken.** Nothing running, store unreachable, a migration refused
  because the database was written by a newer build, scheduled rules not firing, and which layer a status code came from.
  Every setting it names is checked against the settings the source reads.
- `SYNC.md` - the sync server: what a device pushes, what a refusal looks like, and why losing an update has to be asked for.
- `DEVELOPER_GUIDE.md` - working on the codebase day to day.
- `routes-snapshot.json` - the published address set, generated by `cargo run -p xtask -- routes` from the engine's
  routes plus each service's last published list, and held current by a test, so it is a diff rather than a fossil.

## Domains

`domains/` documents each business domain as built: what it owns, the services and gateway prefixes it answers on, and
what it deliberately does not own. The directory contains 29 pages, including proposed work packages.
The command `cargo run -p xtask -- domains` fails if a routed service has no page, if a page names a service or crate
the catalogue does not have, or if a page leaves out one of its own prefixes.

`domains/payments.md` states POS till support only, and that Vimbai Pay is not built, per ADR-005.

## Migration

- `migration/migration-status.md` - **complete.** What moved, how each retiring service was held to its published
  addresses, and the five outcomes that are decisions rather than leftovers.
- `migration/MIGRATION-MAP.md`, `migration/MIGRATION-MANIFEST.md`, `migration/RUST-MIGRATION-BACKLOG.md` - snapshots from
  while it was running. Kept as history, not maintained. The live inventory is `catalog/services/catalog.yaml`; the live gate is
  `cargo run -p xtask -- gateway`.

## Compliance

- `compliance/financial-integrity.md` - the integrity properties the ledger holds, each one naming the test that proves it.

## Not written yet, and why

Who to call: the runbook says what is wrong and what to do about it, but an escalation path is a fact about an
organisation rather than about this repository. Compliance detail beyond financial integrity. Anything belonging to Vimbai Pay or the developer platform, which are out of scope by ADR-005 and
ADR-006.
