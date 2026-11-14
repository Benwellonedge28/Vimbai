# Changelog

Notable changes, newest first. Format follows Keep a Changelog; versions follow semantic versioning.

The project is pre-1.0 while the roadmap in `docs/ROADMAP.md` is incomplete. 1.0.0 lands with the close-out phase.

## [Unreleased]

### Added

- Organisations are a real tenant boundary: `rust/crates/platform/tenancy` owns the organisation, its members and the one
  rule that decides whether a caller may open a Book. A Book may belong to an organisation, its members reach it without
  a membership row of their own, and the engine applies the rule itself (`tenancy_gate`) rather than leaving it to the
  gateway. See `docs/domains/organisations.md`.
- A financial-integrity suite in `rust/tests/financial-integrity`: the ledger's invariants proven from outside the kernel
  and the domains, twelve tests including seeded property tests over random entry sets, plus parity fixtures recording
  the figures two retired Python services answered. `docs/compliance/financial-integrity.md` states each guarantee
  against the test that fails when it stops holding.
- The Flutter feature-first migration started. Authentication - sign in, register, its service and the account it produces -
  moved from `lib/pages`, `lib/services` and `lib/models` into `lib/features/authentication/` with every import rewritten.
  A new gate (`cargo run -p xtask -- flutter`, six tests) fails if a dart file is added to one of the five layer folders
  P7 empties, if the pinned baseline names a file that has since moved, or if any import names a file the app does not have
  - which matters because there is no dart toolchain in CI to notice.
- Opt-in cursor pagination. A list request carrying `limit` or `cursor` answers `items`, `has_more`, `next_cursor`,
  `count` and `total`, keeping the handler's own key for the rows when it had one; a request carrying neither answers the
  same shape it always did, so no client broke. A cursor the engine did not issue, or a nonsensical limit, answers 422.
- Idempotency keys. A write may carry `Idempotency-Key`; the first request with a key is claimed, run and answered, and a
  repeat replays that answer instead of doing the work again - including when the answer was a refusal. A reused key with a
  different request answers 422, a repeat while the first is still running answers 409, and a write with no key is
  untouched. Claims live in the store as platform records and the worker removes the ones older than a day.
- Contracts for every published address. Six hand-written contracts were moved to `contracts/openapi/v1/services/` and
  keep their schemas and prose; `cargo run -p xtask -- openapi` generates one specification per domain in
  `contracts/openapi/v1/domains/` for every address no service contract documents, stating only what the generator can
  know - path, methods, prefix group, shared components - and never inventing a response schema.
  `openapi --check` fails in both directions: an address with no contract, a contract naming an address nothing answers,
  an address documented twice, or a committed file that is not what the generator writes.
  `contracts/compatibility/` states what a client may rely on: the API compatibility policy, the event compatibility
  policy, and the deprecation policy.
- Twenty-two domain pages under `docs/domains/`, one per domain, saying what each crate owns, the services and gateway
  prefixes it answers on, and what it deliberately does not own. The mapping is a gate, not a convention: a routed
  service with no page, a page naming a service or crate the catalogue does not have, or a page leaving out one of its own
  prefixes all fail `cargo run -p xtask -- domains`.
- A worker that actually runs the automations: `rust/apps/worker` claims a due rule from the same store the engine writes
  and runs it exactly once. A run's identity is the rule and the instant it was due, so a crash between claim and
  completion leaves the run to be finished, never started a second time. Completed and failed runs land in the owner's
  inbox and are recorded under `/automation/executions`. `docs/WORKER.md` states the schedule a rule carries.
- The dependency gate gained a `Suite` family (R9): a suite may depend on any crate, and nothing may depend on a suite.
- `cargo run -p xtask -- boundaries`: the crate dependency rules as a gate, with the graph discovered from the
  manifests, one recorded tolerance, and a test that fails if a tolerance goes stale.
- `docs/architecture/system-boundaries.md` and `docs/architecture/dependency-rules.md`.
- `docs/ROADMAP.md`: the ordered plan this repository is worked to, with a measured baseline and per-phase exit criteria.
- `docs/decisions/`: ADR-001 language policy, ADR-002 ledger authority, ADR-003 modular platform, ADR-004 API versioning,
  ADR-005 Vimbai Pay not built, ADR-006 developer platform deferred, ADR-008 directory naming.
- `docs/architecture/` and `docs/migration/` receive the existing layout and migration documents as historical snapshots.
- Repository gates in `rust/xtask`: catalogue generation, gateway address audit, module reachability, plus the
  repository-rules suite.

### Changed

- The thirty-third and last layering defect closed: `vimbai_cost_accounting` reached into the costing domain for one
  function, and `vimbai_advisory` depended on the database adapter to read a string out of a JSON payload. The
  settlement arithmetic now lives in the calculation crate that other code calls, and the payload reader in `common`.
- Documentation index established in `docs/README.md`; contributing guide rewritten for Safe Rust, Go and Dart.

## [0.1.0] - 2026-11-14

### Added

- Rust engine serving all 137 routed services; Go gateway in `gateway/go`; Flutter application in `apps/flutter`.
- `rust/apps/local-runtime`: the on-device runtime, one binary and one data directory, with a file-backed store that
  survives a restart and the gateway's three rules enforced in process.
- `catalog/services/catalog.yaml`: canonical inventory generated from the gateway route table and ownership.
- `rust/xtask`: catalogue, gateway address audit, reachability and repository rules as Rust gates.

### Removed

- All Python: 141 service directories, the pytest suite, the PyO3 extension and its wheel, `scripts/` helpers,
  `vimbai_local/`, `data/migrations`, SDK generation, and every requirements, pytest and lint configuration file.
- Per-service Kubernetes deployments, the 132-image build matrix, and the workflow jobs that ran them.

### Changed

- Ledger integrity decided by `crates/kernel/ledger-core` with exact `Money`; the float tolerance path is gone.
- Journal lines stored as records joined to their entry, linked to their account by edges, with the wire shape unchanged.
