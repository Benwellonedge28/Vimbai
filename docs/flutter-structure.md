# The Flutter app's structure

The app lives at `apps/flutter`. It is the only client: the React `web-frontend/` was dropped as a UI path (ADR on the
frontend), so every screen a person uses is here.

## How it is laid out

One folder per feature under `lib/features/`, each split the same way:

| Folder | What belongs there |
|---|---|
| `presentation/` | screens and the widgets they draw |
| `application/` | what the screen does between a tap and a request; only present once it holds a file |
| `data/` | the service that calls the engine, and the local storage it reads |
| `models/` | the shapes that data has |

Shared by every feature:

| Folder | What belongs there |
|---|---|
| `lib/core/api/` | the HTTP client and its service manager |
| `lib/core/local_db/` | the local database and the signed-in user's local data |
| `lib/core/offline/` | the offline sync queue |
| `lib/core/realtime/` | the live event socket |
| `lib/app/` | the shell: entry point, home scaffold, configuration, the catalogue that maps a Book's services to screens |

The features are `accounting`, `authentication`, `banking`, `books`, `budgeting`, `group_savings`, `household_finance`,
`multimodal`, `npo`, `personal_finance` and `reporting`.

## Why a feature folder and not a layer folder

The app began with `lib/models`, `lib/pages`, `lib/services`, `lib/widgets` and `lib/local_db`. Five folders worked for
fifty-eight files; they do not work for twenty-five features, because adding one screen meant editing four layers and no
folder told you what the screen was for. A feature folder answers that: everything about banking is under
`lib/features/banking`.

## The gate

`cargo run -p xtask -- flutter` prints every feature and the parts it actually holds, then fails on:

A dart file in `lib/models`, `lib/pages`, `lib/services`, `lib/widgets` or `lib/local_db`. Those five folders are what the
migration empties, and they are now empty; a file that appears in one of them is a regression, not a starting point.

An import that names a file the app does not have. This is the failure mode of a file move: the app builds in the editor
that has it open and fails everywhere else. The gate reads every import against the files the app actually has, so this is
caught without a build; and `flutter analyze` now runs in CI as well (see the toolchain section below), which catches the
same fault from the other side.

A pinned baseline entry whose file has moved. `apps/flutter/migration-baseline.txt` exists so the migration could happen
in steps without hiding how far in it was; it is empty now, and it must agree with the folders it describes.

Seven tests hold the gate over the real app, in `rust/xtask/tests/flutter_migration.rs`.

## The toolchain, and where it runs

Dart is one of the platform's three languages (ADR-001), so its toolchain is part of the pipeline that gates a merge and
not an extra that runs when somebody remembers. One workflow installs it, pinned to Flutter 3.50.0: `test-flutter` in
`ci-main.yml`, which runs on every push. It runs four things - `flutter pub get`, `dart format --output=none
--set-exit-if-changed lib test`, `flutter analyze --no-fatal-infos` and `flutter test`. There used to be a second one,
`mobile-analyze.yml`, path-filtered to the app and running those same four steps on the same version; two copies of one
gate only means two places for it to drift, so it is gone and the core pipeline is the one place.

The pin matters more than the version: a `latest` that drifts is a green build that means something different tomorrow
with no commit behind the change. The gate is `workflows::dart_toolchain_problems`, held by two tests in
`rust/xtask/tests/repository_rules.rs`: one asserts this repository passes it, and one builds a scratch estate on disk and
proves the gate fails on a floating `latest`, on two workflows pinning different versions, on a gating pipeline that
installs the toolchain and never runs the tests, and on a second workflow installing the toolchain at all - one gate in
two files is two places for it to drift.

`apps/flutter/test/` holds the app's own tests - nine of them, answering through a replaced transport, so no server and no
emulator. `make flutter-test` runs them locally, and preflight runs them wherever flutter is on PATH, saying so where it is
not.

## What is not done

`integration_test/` does not exist: nothing drives the app against a running engine on a device or emulator. The tests that
exist answer a replaced transport, which holds the offline and wire-shape rules but not the app's screens.

The gates for the platform folders are structural: they exist, are committed, and `flutter build` was not run for each of
them here.

## Tests

`apps/flutter/test/` follows the features: `test/core` for the shared client's offline rules and `test/models` for what
crosses the wire. Run them with `make flutter-test` (which also analyses), or `flutter test` inside `apps/flutter`.
`scripts/preflight.sh` runs them wherever a flutter toolchain is on PATH and prints that it skipped them where one is not,
so a local run never looks greener than it is.
