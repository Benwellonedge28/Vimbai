# Changelog

Notable changes, newest first. Format follows Keep a Changelog; versions follow semantic versioning.

## [Unreleased]

### Added

- One Flutter gate instead of two. `mobile-analyze.yml` ran the same four steps (`pub get`, format, analyze, `flutter test`) on
  the same pinned version as `test-flutter` in `ci-main.yml`, path-filtered so it fired on the pushes that already ran the
  other: two copies of one gate is two places for it to drift, so the core pipeline is the one place. The toolchain gate
  still holds the remaining workflow to a pinned version and to running the app's tests, and now reports a second workflow that
  installs the toolchain at all - one gate in two files is two places for it to drift.
- The gap that deleting the duplicate exposed: `ci-main.yml` declared `paths:` covering Python, Go, Dockerfiles, the gateway and
  `shared/`, and not the app, so `test-flutter` could never start for an app change - a job that is green in the history and
  unreachable from the code it tests. `apps/flutter/**` is in both triggers now, and the gate reads the trigger block itself and
  fails when the workflow that installs the toolchain is filtered away from the app.
- The app's addresses are checked against the platform's. Every screen builds its URL from one constant, and nothing checked
  that the prefix after it was one the gateway routes, so `/budget` sat in the app after the budget surfaces moved to
  `/budgeting` - reachable on a device only because the local runtime forwards what it does not recognise. `xtask
  app-addresses` reads the app's own Dart sources for `${AppConfig.apiUrl}/...` and fails when a prefix is not routed and is not
  one of the two open doors. It found two more, and both are now answered rather than tolerated: the ratios the budget screen
  works out are posted to `/calc/ratio-analysis-ratios`, the calculator that already takes exactly that payload, and the
  household analysis - ported but never registered, so nothing could reach it - is `household-finance-analyze` behind the same
  door. `/budgeting` gained the gateway entry the whole budgeting surface was missing. 139 services.
- ADR-007: Postgres is the system of record for a deployment, SQLite on a device, and Neo4j stays behind the store trait
  as a working backend rather than being extended. It closes the decision P8 was waiting on.
- Moving the data between stores: `vimbai_store::dump` carries every record of every kind and every edge, and
  `vimbai-store-move` puts them into another store and compares the arrival against the source. Edges travel as data, so a
  restore cannot rebuild yesterday's relationships with today's rules. Nine tests, including a move between two different
  implementations and an amount carried intact at thirty-nine digits.
- Two semantic doors on the store trait (`all_edges`, `create_edge`): a relationship is now something the store is asked
  for rather than something a caller infers from fields.
- The catalogue and the published snapshot now say what each service *answers*, not only what the gateway routes.
  `docs/routes-snapshot.json` was written once from a scan of the Python services and had been drifting ever since; it is
  generated now, by `cargo run -p xtask -- routes`, from the engine's own routes plus each service's last published list read
  out of git - the only place a prefix-stripping entry's client-facing addresses survive. 138 services, 1,786 addresses, and
  a test that the committed snapshot is what the engine answers.
- Three real routing defects the address work surfaced, all of the same kind: a door in the manifest that answered
  nothing. `/calcs` was an entry for a service that never had a `main.py`, forwarded to the engine at a path the engine never
  mounted, and the calculator surface it was meant to front - `POST /calc/name` and the catalogue at `GET /calculators` - was
  routed by nothing at all. Both are published properly now.
- Fifteen more entries still forwarded their prefix to the container port their retired service used to listen on, so the
  engine answered the address and the gateway sent the call to a dead socket: a 502 from a door the manifest says is open.
  All fifteen are repointed. Two of them needed the routing fixed as well, and the two needed opposite fixes. `admin-service`
  published bare paths - `/features`, `/audit-logs`, `/dashboard/stats` - that the engine already answers under its own
  registry, so its entry strips the prefix and nothing new is registered. `bank-feed-service` published `/rules`,
  `/transactions` and `/connections`, which alerts, sales-ledger-control and banking-integration already claim as bare paths:
  stripping it would have handed a caller of one service the other's answer, which the stripped-collision gate caught the
  moment it was tried. So its entry keeps the prefix and the engine mounts those routes under `/bank-feed`, once, with three
  HTTP tests over the addresses a client calls. There is no tolerated exception left in the gateway audit: any prefix the
  engine answers that forwards elsewhere fails the build.
- A gate fails if any routed service answers no address, which is the check that would have caught all of the above the day
  the Python service behind each one went away.
- The on-device profile is now what the machine does, not a label on the health door. `vimbai-local` used to mount every
  prefix the manifest routes whichever profile it was told to run, while `/health` reported five members for `core` - so a
  core install answered `/bank-feed` and said it was not serving it. A profile now names the prefixes it serves,
  `addresses::mounts_for`/`mounts_without` split the manifest by that list, and the gateway refuses an address outside the
  profile with a 404 that names the prefix, after the caller has proved who they are. `/health` reports the members actually
  served, which for `full` is all 138 rather than a constant. `identity` came off the core list while doing this: `/identity`
  is an open door in the gateway rather than a routed prefix - a profile that refused to authenticate a caller could not
  serve anything - so it answers on both profiles and was never a member either could withhold. Two tests start the real binary on both profiles: the core one
  answers accounting and refuses the bank feed; the full one answers both.
- The app can tell which machine it is talking to. `/health` now names the prefixes a profile serves rather than only counting
  them, and the app reads that door when it opens: a core install shows one line saying the bank feed, budgets, the cash flow
  statement, document input and live updates are not on this machine, instead of letting a caller tap into a screen that comes
  back 404. A machine that cannot be asked is treated as serving everything - hiding a working feature behind an unanswered
  health check would be worse than offering one that is refused in words.
- The manifest no longer says two things about one address. Fourteen entries stated the container port their retired service
  used while their url forwarded to the engine; `gateway::port_drift` reports any entry whose stated port is not the port its
  address names, and the fourteen are corrected.
- An operator's runbook (`docs/operations/RUNBOOK.md`): the startup line to look for, a bind failure, a worker that exits
  for want of a store, which setting chooses which store, how to test reachability and schema without starting the engine,
  the migration refusal with what to do about it (upgrade - ADR-011 says why), which layer each status code came from, the
  three door settings, and why scheduled rules do not fire. New gate `xtask::runbook` reads the settings the source
  actually asks for and fails if the page names one nothing reads, with a scratch-estate test proving it fails on a
  fabricated name rather than only ever passing.
- The Dart toolchain is in the pipeline that gates a merge, pinned to one Flutter version everywhere (3.50.0): `ci-main.yml`
  now runs `flutter pub get`, a formatting check, `flutter analyze` and `flutter test` on every push, `mobile-analyze.yml`
  runs the same four pinned to the same version, and a new gate (`workflows::dart_toolchain_problems`) fails the build if a
  workflow floats on `latest`, if two workflows pin different versions, or if the gating pipeline installs a toolchain and
  never runs the app's tests. The app's 60 Dart files are formatted, so the formatting check starts green.
- The deployment store's schema is now a numbered step list with a recorded version, the same discipline the device store
  already had (ADR-011). One function decides, for both backends, whether to run the missing steps, run nothing, or refuse
  a store written by a newer build - and refusing is what it does, because downgrading would write the next step against a
  schema the build cannot see. A test compares both stores' steps in order, so a change made in one backend and not the
  other is a failure rather than a surprise; three tests against a real Postgres prove the version is recorded, a record
  written before a second open survives it, a database claiming schema 99 is refused, and every step can be undone and
  re-applied over records that already exist.
- An organisation is the tenant, a Book belongs to one organisation, and one rule decides who may open a Book (ADR-010).
- Every address the engine answers is filed under an owner that can be checked. Three defects in the attribution tooling,
  found by reading what it produced: a handler was attributed from its own line to the end of a 17,000-line file, so
  `/cache-stats` belonged to workflow because the workflow definitions are written below it; a multi-line
  `use budgeting_services::{` and a single-module `use invoicing_services::invoices;` resolved to no crate, leaving
  thirty-four addresses - every budgeting calculation among them - reported as unattributed; and a domain page's address
  table was never read, so a page's own claim counted for less than a crate's. The unattributed bucket is empty and its file
  is deleted, `/calculators` belongs to the calculators page that claims it, and four gate tests pin the rules.
- The worker runs the rules a Postgres deployment holds. It knew a store file and Neo4j, while the engine had already moved
  to preferring Postgres (ADR-007), so beside a Postgres engine the worker started with no store at all and ran nobody's
  schedules - a deployment where the automations quietly stop. "Which store" lives in one place now (`store::choose`), asked
  by every binary that opens one, and a gate fails if a second copy grows back anywhere outside the store crate. The worker's
  half is proven over a real Postgres with Neo4j pointed at a dead address, so a passing run cannot be Neo4j's doing: one
  overdue rule is claimed, run and recorded, and a second pass finds it done instead of posting twice.
- A walk through a Book's list returns each record once, whoever is listing. The first page of a walk was windowed from the
  handler's own order while every page after it was seeked in id order, so when the two orders disagreed the continuation
  either repeated a row it had already sent or stepped over one it never had - intermittent by nature, because it depended
  on the ids that run happened to make. The first page is now a position like the rest (`!`, which sorts before any id),
  which costs one header and makes the whole walk agree. Found while chasing a paging test that failed twice under the full
  suite and passed every time on its own; the test now reads five records two at a time and insists on five distinct ids in
  the store's order.
- The Flutter app has tests, and the toolchain that runs them. `apps/flutter/test/` answers through a replaced transport, so
  the offline rules the whole app leans on are proven without a server or an emulator: a read is retried, a write is not, a
  dead or silent network becomes a typed `OfflineException`, caller headers are sent as given, and a person or a Book
  survives the trip to JSON and back including the fields the engine leaves out. Writing them paid for itself twice:
  `ApiClient` retried by re-awaiting the *same* failed future, so the retry existed in the code and never reached the wire,
  and a socket failure on a read was not retried at all. `make flutter-test`, the mobile workflow and `preflight.sh` (where
  flutter is on PATH, and it says so where it is not) all run them. The six platform folders are generated and committed, so
  a clone can build for android, ios, web, linux, macos or windows.
- Every address the engine answers is filed under the domain that answers it. The catalogue describes what the gateway
  *routes*, and retiring a service moved its addresses onto the engine without moving them in the gateway, so 180 prefix
  groups were answered by nothing the catalogue knew about and the contract generator put them all in one bucket called
  `unattributed`. Ownership is not missing, it was written in another place: the route names its handler, the handler names
  the crate it delegates to, the crate is documented by a domain page. The new `xtask/src/owners.rs` reads exactly that, so
  a new address is attributed by being written like every other address and a handler that stops delegating stops being
  attributed; `cargo run -p xtask -- owners` prints the table and reports a prefix two domains answer instead of settling it
  quietly. `unattributed.yaml` is gone, every published address now sits in a domain specification, and four gate tests hold
  it: no address unattributed, every owner a real crate answering real addresses, a known door pinned to the crate its code
  calls, and a split prefix reported rather than guessed.
- Cursor paging no longer stops at a handler's cap. A cursor used to be an offset into the answer the engine had already
  built, so a Book holding more records than one listing was willing to produce could not be walked past that cap. A cursor now
  also names the row it reached (`v1:{offset}:{id}`, and the old `v1:{offset}` still decodes), the paging layer puts that place
  on the request it forwards, and the store seeks: every backend answers `store_list_after` in id order - memory by filtering
  what it holds, SQLite and Postgres by asking the database, the device file store through the memory graph - so the cap bounds
  one answer instead of the whole walk. A handler that does not list through the store is detected (its rows include ids at or
  below the cursor) and windowed by count as before, so nothing that worked stopped working. Proved by writing 1,200 rules into
  a Book whose listing answers at most 1,000 and walking every page: all 1,200 reached, none of them twice.
- Every server this repository ships now serves behind the same door: the engine (`governed_router`), the on-device
  runtime and the sync server all take `api::governed`, which reads `VIMBAI_API_MAX_IN_FLIGHT` and `VIMBAI_API_TIMEOUT_MS`
  in one place rather than each binary having its own idea of the numbers. A new gate (`cargo run -p xtask -- boundaries`'s
  sibling `xtask/src/servers.rs`) fails when a `main.rs` that calls `axum::serve` has not wrapped its router in it, and it
  asserts it actually found the servers rather than passing over none. A test sends thirty-two callers at an on-device
  runtime at once and every one is answered, so the layer is in the path rather than in a comment.
- Safe Rust is now enforced across the whole repository, not only at crate roots. The sweep checks both `src/lib.rs`
  and `src/main.rs` of every crate (a crate that ships a binary and a library is two crates to the compiler, so an
  attribute on one says nothing about the other), gives all 83 integration test crates their own
  `#![forbid(unsafe_code)]` since each compiles separately, and scans every `.rs` file for an actual `unsafe` block while
  ignoring the string literals a gate uses to state its own rule. Zero files use unsafe; the gate now keeps it that way.
- A deployment engine written to at once: sixty-four callers write through the real binary against a real Postgres, with
  the door admitting four and the pool holding two, while eight readers keep asking for the Book. Every write lands with
  its own id and nothing is lost. The store's own concurrency tests could not show that path; this one does.
- The engine now has a door: it admits a bounded number of requests at once and gives each a deadline, both settings
  (`VIMBAI_API_MAX_IN_FLIGHT`, default 256, and `VIMBAI_API_TIMEOUT_MS`, default 30s) read from the environment. A burst
  queues for a slot instead of piling onto the store's small pool, and a call that cannot start or cannot finish in time
  gets a named answer (503 with the reason, 504 with the deadline) rather than an open connection. Worker threads are a
  setting too (`VIMBAI_API_WORKERS`, default one per core).
- A page can be read one record at a time: `Accept: application/x-ndjson` answers a collection as one JSON record per
  line, written as the engine hands each row over, with the paging facts in headers (`x-total-count`, `x-page-count`,
  `x-has-more`, `x-next-cursor`). Asking for JSON gets exactly what it got before - the convention is opt-in, as ADR-004
  requires.
- The engine on Postgres, proven rather than configured: three tests hand the router a Postgres store and write, read
  back, keep two Books apart, and find the record after the store is reopened, then check which store is chosen when both
  Postgres and Neo4j are configured.
- A deployment store now closes its pool on a thread of its own. Its client's destructor drives a runtime, so dropping one
  inside the engine's runtime aborted the process rather than unwinding - the difference between a clean shutdown and a
  crash at exit.
- Both local binaries report the port they actually bound, and their tests read it back instead of reserving a port and
  hoping nothing takes it in between. That race, not a defect in either server, is what failed one on-device test while the
  whole workspace was running beside it.
- Postgres as the deployment store, behind the same semantic doors. Eleven tests against a real Postgres 15: Book
  isolation, an entry and its lines in one transaction, a refused posting leaving nothing behind, a thirty-nine digit
  amount exact, shared rows, twenty-four concurrent writers losing nothing, four Books at once, and a Book carried from
  SQLite into Postgres and compared on arrival. The engine prefers `VIMBAI_POSTGRES_URL` to Neo4j, which stays reachable
  for anyone still running it. Two defects the fake could not have shown were fixed on the way: the synchronous client
  panicked inside the engine's async runtime ("cannot start a runtime from within a runtime"), so each operation now runs
  on its own thread, and a `jsonb` parameter sent as text was refused, so the record travels as a value. `store-move`
  learned `sqlite:` and `postgres:`, and preflight starts a throwaway Postgres when the server binaries are installed.
- SQLite as the device store, replacing the JSON file. It answers the semantic doors natively, with a real schema, WAL
  and numbered migrations that each roll back; an entry and its lines arrive in one transaction, a refused posting leaves
  nothing behind, and two hundred concurrent writers lose nothing. Shared rows (memberships, sync batches, organisation
  links) gained their own doors and are native too. The on-device runtime opens `store.db`, and an existing `store.json`
  is carried across once on first boot and kept, renamed, beside it. Eleven device-store tests and the five end-to-end
  ones, now running on SQLite.
- The store trait is semantic: seventeen doors for the operations the platform performs, and `Records` and
  `PlatformRecords` use them, so no caller hands the store a query any more. A backend that refuses `run` and answers only
  the doors serves every caller, which is held by a test rather than asserted, and a gate fails if a caller goes back to
  writing statements.

## [1.0.0] - 2026-11-14

The roadmap in `docs/ROADMAP.md` is complete: every phase has landed, and the two things deliberately out of scope -
Vimbai Pay and the developer platform - are decisions with ADRs rather than gaps.

### Added

- The scale suite (`rust/tests/scale`, `vimbai_scale`): what the platform does at magnitudes and concurrency it was not
  written against. Forty-digit amounts stored and read back unchanged; twenty thousand thirty-digit addends totalling
  identically in either order; a thirty-digit amount allocated across a thousand parts adding back to the whole to the
  last minor unit; sixteen and then a hundred writers on one Book losing nothing, with exact totals, Book-wide, and large
  amounts intact under contention. Its helpers are library functions, not test-local code, so a later suite hammers the
  store the same way rather than writing a second summing rule.
- ADR-009 records the one tolerated dependency: the kernel's reading half reads the store, while the posting authority
  stays store-free. The gate cites it, and a test refuses an exception citing an ADR that does not exist.
- `docs/architecture/overview.md` rewritten from the finished tree, and `docs/migration/migration-status.md` set to
  complete with the five permanent outcomes stated.
- Infrastructure is one story per environment: `infrastructure/docker` holds one Dockerfile for the three Rust
  binaries (engine, worker, sync) plus the gateway and the Flutter web app; `infrastructure/compose` is the estate with
  dev / test / observability layered on top of it rather than repeated; `infrastructure/kubernetes` is `base/` plus
  overlays for development, staging and production. The Helm chart is deleted: its values still listed forty retired
  services and its templates ranged over them, so kustomize is now the only deployment path. `cargo run -p xtask --
  manifests` holds the cluster, the compose estate and the images to the repository, refusing a compose build that names
  a package or binary the workspace lacks, a kustomization naming a file that is not there, a manifest loose at the top
  of the cluster tree, and an image built by nothing.
- Organisations are a real tenant boundary: `rust/crates/platform/tenancy` owns the organisation, its members and the one
  rule that decides whether a caller may open a Book. A Book may belong to an organisation, its members reach it without
  a membership row of their own, and the engine applies the rule itself (`tenancy_gate`) rather than leaving it to the
  gateway. See `docs/domains/organisations.md`.
- A financial-integrity suite in `rust/tests/financial-integrity`: the ledger's invariants proven from outside the kernel
  and the domains, twelve tests including seeded property tests over random entry sets, plus parity fixtures recording
  the figures two retired Python services answered. `docs/compliance/financial-integrity.md` states each guarantee
  against the test that fails when it stops holding.
- A sync server exists: `rust/apps/sync-server` serves the Book surface - sign in, push, pull, devices, membership - from
  the engine's own handlers behind a door that answers 404 to everything else. It applies the cluster gateway's rules
  itself, which meant moving those rules into `crates/adapters/gateway` so the on-device runtime and the sync server share
  one copy rather than two.
- Conflicting writes are refused rather than silently resolved: an entry may declare the record and version it was made
  against, the whole batch is decided before it is written, and a moved ledger record is answered 409 with the record's
  name and both versions while a note or draft goes last-write-wins and says so. A refused batch stores nothing, so a
  device is never left half-synced. Pushing twice is still free: the answer now reports `already_stored` too.
- A paged answer keeps the handler's own fields beside its rows. Paging a Book's entries had dropped `book_seq`, which is
  the high-water mark a syncing device quotes back.
- The Flutter migration finished. All 58 dart files left `lib/models`, `lib/pages`, `lib/services`, `lib/widgets` and
  `lib/local_db`: eleven feature folders under `lib/features/`, plus `lib/core/` for the api client, local database, sync
  queue and event socket and `lib/app/` for the shell. Every import was rewritten and the gate proves each one still names a
  file the app has. `apps/flutter/migration-baseline.txt` is empty, and `docs/flutter-structure.md` records the layout.
- The dart build cache (`apps/flutter/.dart_tool`) is no longer tracked; it is build output, and a tracked build cache is
  a merge conflict waiting to happen.
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
