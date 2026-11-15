# Changelog

Notable changes, newest first. Format follows Keep a Changelog; versions follow semantic versioning.

## [Unreleased]

### Added

- The app can set up an organisation, which is what makes a Book shareable. The engine has answered `/identity/organizations`
  all along, but nothing in the app could reach it, so a person who registered on a phone had an account and a Book that
  belonged to nobody and no way to hand it to an accountant or a partner. There is now a service (list what you belong to,
  open one, put a Book inside it, list its Books), a setup page reachable from the home screen, and the engine's own
  refusal wording kept so a member who cannot move a Book is told why rather than shown a status number.
- A Book's own settings now say whether it is shared. The Sharing row names the organisation it is inside, says plainly when
  it is in none ("only you can see this Book"), and opens the same offer sheet to change it - so a decision made in the wizard
  can be revisited. When the server cannot be asked the row says nothing at all, because "private" and "unknown" are not the
  same thing and only one of them is true.
- A Book that has just been made offers to go into an organisation straight away, which is the moment it would otherwise
  become a private dead end. Only organisations the person can actually put a Book into are listed (offering a refusal as an
  option is a lie), even a single organisation is asked rather than assumed, and the whole offer is best-effort: offline, the
  Book still exists, and nothing is written about a server that did not answer.
- From the organisation page a person can put a Book that is on their device into an organisation, which is the step that
  actually makes it shareable. The Books already inside are left out of the offer, both lists are asked for at the moment of
  the tap (a Book may be newer than the page), and the engine's refusal is shown rather than retried.
- The organisation page tells "you have none yet" apart from "the call failed". A failed read leaves the list as empty as a
  real one, and a page that says you own nothing because the server did not answer sends people off to open a second
  organisation they already have. Twenty-four app tests cover the service, the three states of the page, putting a Book in, and the share offer shown when a Book is made.
- Passwords follow NIST SP 800-63B instead of a length limit and nothing else. Eight characters minimum counted as
  characters rather than bytes, so a password written in Amharic or Devanagari is not refused for being shorter than it
  looks; no composition rules and no expiry, because those push people towards `Password1!` and `Autumn2024!`; the
  commonly tried passwords refused; a single repeated character refused; and the account's own username or email refused as
  its password. One module (`vimbai_sso::password`) holds the rules, and registration, password change and password reset
  all go through it, so there is one answer to "what may a password be" rather than three.
- Unicode is normalised (NFKC) before hashing, so the same password typed on a phone keyboard and a laptop is one password.
  Checking tries the stored form first and the normalised form second, so every hash made before this still verifies.
- bcrypt now runs at cost 12 with `needs_rehash`, and a hash made at an older cost is replaced on the next successful
  sign-in. That is the only moment a password is in the clear, so it is the only moment the upgrade is free; nobody is
  forced to change anything and no account is locked out.
- Sign-in is throttled. Five wrong guesses put that identifier aside for fifteen minutes, answering 429 with `retry_after`
  and a stated wait, and a correct password clears the count. It is counted whether or not the account exists, so the
  throttle cannot be used to find out which usernames do.
- A weak password is refused with the reason in the person's own words ("Use at least 8 characters", "That password is one
  of the most commonly used") rather than "requirements not met", which leaves them guessing which requirement.

### Fixed

- A wrong password opened the app. Two fallbacks stood between the engine's refusal and the person: the sign-in page turned
  any failed sign-in into a session whenever a profile was already cached on the phone, and the auth service, when the
  phone reported no connection, admitted any username whose profile was there. Registration had the same shape - with no
  connection it invented a local account that existed nowhere else and returned success, so the person believed they had
  registered and could never sign in from another machine. All three are gone: the engine checks the password on every
  sign-in and creates every account, which it can do on a device because the engine runs locally, and an engine that cannot
  be reached now says so instead of admitting anyone. Cached data on the phone is for reading what is already there, not
  for deciding who somebody is.
- The app's shared HTTP client did not recognise a refused connection. `package:http` reports that as its own
  `ClientException`, not as a `SocketException`, so every page that handles being offline got an unhandled error instead -
  which is the shape a dead engine takes on a phone. It now maps to the offline answer like the other two, and a test
  covers it.

### Added

- `cargo run -p xtask -- flutter` now fails when a file under the authentication feature reads `connectivity_plus`. Who
  somebody is, is the engine's answer; a phone's connection state says nothing about a password.

- A Book full of posted entries reported nothing. The on-device file store and the in-memory store each returned a listed
  entry one level down, so trial balance, income statement and balance sheet read a record's outer wrapper and counted
  zero: a posted sale of 250 read as 0 debit and 0 credit. Both now read the entry itself, and the sale reads 250 and 250,
  balanced. Postgres was already correct, which is why this only appeared on a device.
- A new Book could not be written in. A journal entry may only name accounts its Book owns, so the first person to post
  after opening a Book was told the account they named did not exist and had to build a chart before recording a sale.
  Opening a Book now seeds a standard chart from the accounting domain, chosen by the Book's tier (business, personal,
  household, group, nonprofit, and the core chart for a tier the engine does not know). Seeding never renames an account
  somebody has already edited and never adds a number the Book already holds, so running it twice adds nothing.
- Books and accounts came back undated. `created_at` and `updated_at` were whatever the caller sent, which was usually
  nothing, so a listing ordered by `created_at` had nothing to order by. Both are now stamped in one place, in the store's
  record funnel, and only when the caller states no moment: a caller that names a time still keeps it.

### Added

- `rust/apps/api-server/tests/first_run_http.rs` walks the path a stranger walks - register, sign in, open a Book, post an
  entry, read the balance - against the real engine, and pins that a draft entry is kept but moves nothing.

- The roadmap's Evidence column was unchecked: it cited `tests/parity.rs` and `request-conventions.md` without the folders they
  sit in, claimed 11 tests in the worker's `src/lib.rs` where there are 12, and named a deleted file as evidence. The roadmap
  check now reads the evidence column too - a cited path has to resolve, from the repository root, from one of the trees the
  document names, or from a path cited earlier in the same cell, and a stated test count has to be the number of `#[test]`
  functions in the file it names. Four citations were corrected; a line recording a removal is still allowed to name what was
  removed. A test runs the check over a table written to be wrong, so the gate cannot pass by scanning nothing.

- The roadmap's status table carried figures nobody checked, and had drifted: it said 139 routed services while the catalogue
  held 143, and 2,910 tests long after the suite had grown past it. `cargo run -p xtask -- roadmap` compares every figure the
  tree can answer for - routed services, domain, calculation and platform crates, Python files, catalogue domain files, dart
  files under `lib/` - and fails on a disagreement, and it fails if the table is rewritten so those seven facts are no longer
  there to check. The two figures only a run can produce (the test count and the gate-test count) are gone from the table rather
  than stated from memory. The Python count asks `safety`, which already answers that question for the safe-Rust rule, instead
  of scanning the tree a second time.

- A prefix group answered by more than one crate - `/records` is answered by seven - was filed under the crate that answered
  most of its addresses, with a tie going to whichever the scan reached first, so the same repository could file the same prefix
  differently on another machine. The choice is a function of its own now, ties are settled by name, and a test pins all three
  answers: most addresses wins, a tie goes to the smaller name, and a group nothing answers is filed nowhere instead of being
  guessed at. About 180 prefix groups that had no owner at all are already attributed, so this is the last gap in the filing.

- Four surfaces were published on addresses no client asks for. `/invoices`, `/customers`, `/reports` and `/tasks` are what the
  app and its callers are written against, but the catalogue carried no row for any of them: the retired services are published
  through wildcard doors (`/invoicing`, `/reporting`, `/multimodal`) that only reach them by repeating a prefix the caller did not
  ask for. Each is a catalogue entry now, named on its domain page, with contracts and owners generated from the code that answers
  it; the routed surface goes from 139 to 143 and the wildcard doors stay, because they publish a retired service's whole surface
  and the address audit reads them against it. Two gates were leaning on figures that could go stale: the routes snapshot asserted
  a hand-written 139 while the catalogue had already grown past it, and it now reads the catalogue's own count, so the check fails
  on staleness instead of on growth.

- The gateway had two routing tables. One was `config/services.json`, which the engine, the on-device runtime and the audit all
  read; the other was twenty-two routes written in Go beside it, each with its own `*_SERVICE_URL` in the manifest, each pointing
  at a service retired into the engine. Clients calling `/accounts`, `/budgets`, `/customers` or `/banks` reached hosts that do not
  resolve, while `/accounting/accounts` reached the engine that answers the same surface. No check could see it: the address audit
  reads the manifest, and those routes were not in the manifest. The table is now one route, `/identity`, because signing in is how
  a caller becomes somebody and cannot require a token. `cargo run -p xtask -- gateway` reports any self-declared route the engine
  does not answer, so a door to nothing fails the build, and the gateway README now documents the addresses that exist instead of
  the retired services' shapes.
- The complement of the audit's question is now asked too. The audit reads a stripping entry - one that forwards `/prefix/rest` to
  the engine's `/rest` - against the endpoints its retired service last published, so it covers the shape whose target is decided
  per request. Nothing covered the other shape: an entry that forwards unstripped has to be an address the engine answers itself, and
  that is now asserted for every such entry. (The first draft of this reported twenty-six broken prefixes. They were all stripping
  entries, judged by a rule that could not see them: the check mis-filing a shape, not a defect. Both shapes are covered now, and the
  test says which one it is judging so the mistake is not made twice.)

- A manifest could configure something nothing reads, and every file involved looked right. Three of them were live: the sync
  server was handed `VIMBAI_SYNC_ADDR` and ignored it, so the pod bound `127.0.0.1:8100` while declaring container port 8099 -
  a sync endpoint no client in the cluster could reach; the engine was handed `OTEL_EXPORTER_OTLP_ENDPOINT` with no
  OpenTelemetry exporter linked into any binary, while `compose.observability.yaml` said tracing came on with no rebuild, so the
  collector received nothing; and a set of service URLs pointed at services that no longer exist. A new gate (`make settings`)
  fails when a manifest or compose file sets a name no source file reads, with a named excuse - and the image that reads it - for
  the variables belonging to images this repository pulls rather than builds. The excuse list is checked too: an entry that
  stops being configured, or that hides a name our own code reads, fails the build. `VIMBAI_SYNC_ADDR` is now read the way the
  engine reads `VIMBAI_API_ADDR`, an empty value treated as none, and the tracing claim is written as what the binaries do.

- The engine and the sync server signed tokens with different secrets. The engine read `VIMBAI_JWT_SECRET` then `JWT_SECRET`;
  the sync server ignored both, minted a secret on first boot and kept it in a file beside its data. A deployment that set the
  variable therefore had two components issuing and checking the same tokens against two keys, so a token minted on one side was
  refused on the other - and the on-device runtime, which sets neither, never showed it. One rule now lives in `vimbai_common`
  (`secret_from_environment`), both components call it, and a machine with nothing configured keeps the file it needs to start.
  A test starts the real sync server with the variable set and verifies the token it signs against that secret and against a
  different one, so the answer cannot come from a written or generated value.

- The estate pointed at a database it did not have. `calculation-api` read `NEO4J_URI` from a ConfigMap, the kustomization generated
  the same ConfigMap name with a different host (`neo4j` against `neo4j-service`), no manifest declared a Neo4j at all, and the
  worker read `neo4j-password` from a Secret that declares `NEO4J_PASSWORD`. Each file looked right; together they were a cluster
  that starts, mounts its record routes, and fails at the first query - with a green deploy. Per ADR-007 the deployment store is
  PostgreSQL, so the engine and the worker now read `VIMBAI_POSTGRES_URL` from `vimbai-secrets`, the hand-written ConfigMap is gone
  (the generator was already declaring the same name), the sync server reads the `JWT_SECRET` the Secret actually declares, and the
  Neo4j backup CronJob, its volume and its network policies are removed - they maintained and protected a database the estate never
  ran. Compose keeps its Neo4j, because compose declares one.
- `cargo run -p xtask -- manifests` now compares both directions: a Secret key a container reads has to be declared by that Secret,
  and a store address written into a manifest has to name a Service the estate serves. A placeholder containing `CHANGE_ME` is
  skipped, so the DSN an operator replaces does not have to name a Service the repository cannot provide.

- Both deploy workflows restarted one Deployment by name - `api-gateway` - and waited for that one. Every image carries
  `:latest`, so `kubectl apply -k` on an unchanged spec starts no rollout: the engine, the sync server and the worker were applied
  on every release and kept serving the previous image until a pod happened to die. A green log cannot tell a restart of one
  Deployment from a restart of the set. Deploys now go through `scripts/deploy.sh`, which applies the overlay, reads the Deployments
  from the cluster and restarts and waits for every one of them, so a fourth Deployment rolls out because it exists rather than
  because someone remembered a list.
- `cargo run -p xtask -- deploy` (`make deploy`) refuses a workflow that restarts a named Deployment or applies an overlay without
  the script, and refuses a script that has gone back to naming its deployments instead of asking for them.

### Added

- `docker-publish.yml` had never published an image. It found "changed services" by looking for a top-level directory holding a
  Dockerfile - the shape the Python microservices had - and no such directory exists here, so every matrix entry set
  `exists=false` and skipped itself: a green job, beside four Deployments asking ghcr.io for images nobody had pushed. The engine
  image was built by a second workflow, `calculation-api.yml`, which is why nobody noticed the first one doing nothing. There is now
  one workflow publishing the four images the cluster pulls, from the three Dockerfiles that exist, with the Dockerfile named in the
  matrix so a missing one fails the build instead of being skipped past; `calculation-api.yml` is deleted rather than left racing it
  to push `:latest`. The gateway's manifest asked for `vimbai/api-gateway` on Docker Hub, which no credential here can push to, and
  now asks for the same ghcr path as the other three.
- `cargo run -p xtask -- images` (`make images`) keeps the two halves comparing: every image a manifest or compose file pulls has to
  be published by a workflow, every image published has to be pulled by something, and every Dockerfile a build step names has to
  exist. A third-party image is recognised by its registry rather than listed, so a new `postgres:16` needs no edit.
- A `needs:` that names a job that no longer exists is answered by GitHub rejecting the whole workflow file: no run at all, no
  error anyone reads, the pipeline simply stops existing. That is how `ci.yml` died - three jobs still needing `lint-python` and
  `test-python-services` after the Python jobs were deleted - and nothing local caught it, because nothing local reads
  `.github/workflows`. `cargo run -p xtask -- workflows` (`make workflows`) now resolves every `needs:` against the jobs declared in
  its own file, keeps every local `uses:` path existing, and fails a file that declares no jobs at all, which is the second way a
  workflow goes silent.
- A crate the workspace does not list is invisible, and nothing was watching for one. `cargo test --workspace` would never build
  it, clippy would never read it, and `make safety` would hold it to nothing; it compiles only if someone remembers to cd into it.
  `cargo run -p xtask -- workspace` (`make workspace`) compares the manifest against the crates on disk in both directions: every
  `rust/**/Cargo.toml` is a member or in `exclude`, and every member is there. A crate declaring its own `[workspace]` is allowed
  only when it says so in `exclude`, because that is the one case where not building it from here is deliberate. Today the two
  lists agree exactly: 70 members, 70 crates.
- A clean preflight now means a green pipeline, and it did not. `ci-main.yml`'s app job has run `dart format
  --set-exit-if-changed` and `flutter analyze --no-fatal-infos` since it was written; `scripts/preflight.sh` ran `flutter test`
  alone, so an unformatted or unanalysed change passed preflight and failed the pipeline at the first step. Preflight now runs all
  four of that job's steps, and `cargo run -p xtask -- preflight` (`make preflight-check`) fails when a pipeline runs a
  verdict-reaching command - `cargo`/`go`/`dart`/`flutter` with `test`, `clippy`, `vet`, `analyze`, `fmt` or `format` - that the
  script does not. Preparing a toolchain (`flutter pub get`, `go mod download`) is not a verdict and is not demanded of it.
- The two standing rules nothing was checking are now a command and a test. Safe Rust only, and no Python, were enforced by the
  deleted pytest suite; both are true today, which is the state a rule stops being true in - a new integration test crate without
  `#![forbid(unsafe_code)]` builds, passes its own tests and quietly brings back the ability to write `unsafe` in that file.
  `cargo run -p xtask -- safety` (`make safety`) checks every crate root - library, binary, build script, and each integration
  target, because an integration test is a crate of its own - then reads every `.rs` for a real `unsafe` block and every file for
  Python by extension or project name. Reading only what can be governed keeps prose out: `unsafe {` inside a string literal is a
  gate stating its own rule, not a use of one, and the string stripper is now shared with the crate scan rather than copied. The
  tree walk it needs became `repository::files`, replacing the two private copies.
- The proofs a page names have to exist. `docs/compliance/financial-integrity.md` opens by promising that "every claim names the
  test that fails if the claim stops being true", and until now nothing checked the names: renaming one of those tests would have
  left the promise standing with nothing behind it. `documentation::named_tests_exist` reads every `tests/<file>.rs::<test>`
  reference under `docs/` and requires the file and the function, which is decidable because Rust defines a test by `fn`. Writing
  it found `tests/parity.rs` is not one file - the engine and the integrity suite each have one - so a name is satisfied if a file
  of that name defines it. The test pins both directions and refuses to pass over an empty set: it counts the named tests in the
  integrity record and fails if it finds fewer than it should be reading.
- The scripts are held to the same reference rule as the pages, and `scripts/preflight.sh` was the reason. It told every reader
  that where flutter is not on `PATH` the app's tests "run in the mobile workflow" - a workflow deleted when `mobile-analyze.yml`
  was folded into `test-flutter`, so the line named a job that no longer runs. It now names the `test-flutter` job and
  `ci-main.yml`. `documentation::stale_references` also reads `scripts/*.sh`, where a file name is written without backticks, and
  a shell comment is prose worth checking - a markdown `#` is a heading, a shell `#` is a sentence.
- A dead reference in the documentation fails a build. `docs/ROADMAP.md`, the working plan, cited
  `docs/decisions/ADR-005-vimbai-pay-deferred.md` for as long as it took anyone to notice the decision had been recorded as
  `ADR-005-vimbai-pay-not-built.md`; the reader following it got nothing, in the one document that says what to build next.
  `documentation::stale_references` now checks every page under `docs/`, plus the front page: a decision cited by its file name
  has to be a file under `docs/decisions/`, and a workflow named by file has to exist somewhere in the repository. A line that
  records a removal is exempt, because naming what was deleted is how a reader learns it is gone - `flutter-structure.md` keeps
  saying `mobile-analyze.yml` is not there any more.
- The Makefile is checked like the documentation is. `flutter-test` was never declared `.PHONY`, so a path of that name would
  have made the target silently do nothing, and `make help` - the file's only index - described eight targets while the
  repository's gates had grown to eleven commands, so the gates a reader could not see looked like ones that did not exist.
  Both are fixed, `make help` now lists every target, and the gates gained `make docs`, `make readme`, `make makefile`,
  `make routes`, `make openapi`, `make owners`, `make domains` and `make manifests`. `makefile::makefile_claims`
  (`cargo run -p xtask -- makefile`) fails on a target that is not `.PHONY`, a target `make help` omits, a help line for a
  target that does not exist, or an `xtask` command the binary does not implement - and it knows `xtask --manifest-path` is a
  cargo flag rather than a command.
- The documentation index is checked instead of trusted. `docs/README.md` said it existed so nobody had to guess "which of
  ten documents is current" while there were eight, called the domain pages twenty-two while `docs/domains/` held 24, left
  `SYNC.md`, `flutter-structure.md`, `architecture/DUPLICATION.md` and `compliance/financial-integrity.md` out entirely, and
  pointed at `MIGRATION-MANIFEST.md` and `RUST-MIGRATION-BACKLOG.md` in the wrong directory. `documentation::documentation_index`
  (`cargo run -p xtask -- docs`) now holds it to four things: every document under `docs/` is named (a page inside a folder the
  index documents is accounted for by the folder), every path it names exists, every `cargo run -p xtask -- <command>` it tells
  a reader to run is an arm of the binary's own dispatch, and a figure printed beside the domain pages is the number of pages
  there are.
- The front page stopped claiming things the repository decided against. `README.md` described "a decoupled microservices
  architecture", "a Graph Database (Neo4j Community Edition)" used for native traversal, and integration with "payment
  gateways". The platform is one Rust engine behind one Go gateway with 139 routed surfaces and a catalogue that proves it;
  the store is Postgres for a deployment and SQLite for a device, with Neo4j still working behind the same trait and
  deliberately not extended (ADR-007); and Vimbai Pay was decided out of scope before it was written (ADR-005), so POS
  integration ships and a card gateway does not. New gate, `readme::readme_claims` (`cargo run -p xtask -- readme`): a figure
  printed beside "routed surfaces" has to be the catalogue's own count, and every `ADR-0NN` the front page cites has to be a
  decision that exists.
- `ci.yml` is deleted, and it had not been running anything. Three of its jobs still listed `lint-python` and
  `test-python-services` under `needs` after those jobs were deleted with the Python services, so GitHub rejected the whole
  file and its CodeQL, Trivy, Grype, ZAP, SBOM and failure-notifier jobs had not executed since. Every job it had also carried
  `continue-on-error: true`. What was worth keeping moved to `security-scan.yml`: `cargo audit` over the lock file the engine
  ships, and CodeQL over the gateway (retargeted from `python, javascript, typescript` to `go`). What it duplicated is covered
  elsewhere: Go build and test by `ci-main.yml`, image builds by `docker-publish.yml` and `calculation-api.yml`, deploys by
  `cd-production.yml`. Its ZAP job scanned `https://www.example.com` and its Grype job scanned the same filesystem Trivy
  already covers, so neither is missed. The deploy job it carried is the one thing not restored anywhere: it applied the
  production overlay on every push to `main`, a second untagged door into production beside the release workflow.
- The pipeline index describes the pipelines that exist. `.github/workflows/README.md` still described a Python matrix over
  300+ microservices, `ruff` and `bandit`, Codecov, Helm and Terraform - none of which is how this repository works - so the one
  page that says what gates a change was telling readers about gates that do not exist. It now names each workflow, when it
  starts, and what it decides, including the split between `library-ci.yml` (Rust), `ci-main.yml` (Go and the app) and the
  legacy all-in-one `ci.yml`. New gate: `workflows::documented_workflows` fails when a workflow is not named in the index, or
  when the index names one that has been deleted.
- `iac-validate.yml` no longer carries a Terraform job. There is no Terraform in the repository, and the job ran
  `terraform fmt -check ... || true` behind an `if [ -d infrastructure ]`, so it could not fail and did not check anything.
- The gateway's tests can fail the build now. Both places that ran them wrote `go test ./... || echo "No tests found"`, so a
  failing test printed a line and the job went green: the Go half of the platform had a suite that had never decided anything.
  The `|| echo` is gone from `ci.yml` and `ci-main.yml`, the gateway's two test files pass as they stand, and preflight runs
  `go vet` and `go test` where a Go toolchain is installed (saying so where it is not). A gate,
  `workflows::swallowed_test_failures`, reads every workflow for a step that runs `go test`, `cargo test`, `flutter test`,
  `dart test` or `npm test` and then swallows the verdict with `|| true`, `|| echo` or `|| :`.
- The deployment-store tests now run in CI instead of skipping there. They answer against Postgres or they skip, and a skip
  reads exactly like a pass: `library-ci.yml` had the variable's tests and no server. It starts a `postgres:16` service with a
  readiness check and names it in `VIMBAI_TEST_POSTGRES`, the same shape preflight gives them on a laptop. A gate,
  `workflows::postgres_test_problems`, reads which test files look for that variable and fails when the pipeline that runs the
  workspace suite neither provides it nor starts a server to reach - and fails when nothing looks for it at all, so the scan
  cannot pass over an empty set.
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
