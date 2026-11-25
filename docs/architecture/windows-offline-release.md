# Ship an offline Windows build with the local accounting engine

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Depends on the access and portable recovery contracts in 03 and 05 and the release provenance rules in 06.

## Existing Windows entry points

`apps/flutter/windows/CMakeLists.txt` and `apps/flutter/windows/runner/main.cpp` provide the Flutter shell. `apps/flutter/lib/app/config.dart` selects the API origin at compile time. `apps/flutter/lib/core/local_db/database_helper.dart` opens the Flutter cache and outbox through sqflite; the Windows runner must prove that its production database factory supports those calls. `rust/apps/local-runtime/src/main.rs` starts the same accounting API with a SQLite store, accepts `--port 0`, and reports the chosen address as text. `scripts/make_offline_bundle.sh` describes a standalone runtime on Unix. Its output does not assemble the Windows desktop app.

## Build and startup contract

Add one Windows x64 workflow under `.github/workflows/`. From the same pinned commit, compile the Flutter release and `vimbai-local.exe`, retain the Release directory with its DLLs and assets, and produce a ZIP with checksums and a manifest listing OS, architecture, app version, runtime version, API compatibility version and required Visual C++ runtime. Keep the app and engine versioned as a unit. Smoke-test the extracted release directory before signing and publishing. A future installer must preserve the user data directory across updates.

Introduce a Flutter desktop launcher behind the existing application configuration. In standalone mode, start only the bundled executable using an absolute path and fixed arguments. Add an opt-in machine-readable startup message to the runtime with the bound port and protocol version. Bind explicitly to loopback and accept a ready endpoint only from the spawned child and only at the reported loopback origin. Pass the resulting origin to the existing API client. Keep standalone and hosted sessions in separate stores with clearly labelled Book selectors. Use a production Windows SQLite adapter for any surviving Flutter outbox/cache paths. The Rust store remains the local ledger authority, and an action submitted to it must not also enter the hosted outbox automatically.

## Data, failure and recovery

Use a per-user data location outside the installation folder and verify Windows access controls for the runtime secret, database and attachments. The existing `secret_in` restricts permissions using a Unix-only branch, so Windows permissions need their own verification. Take a usable backup before a schema upgrade. Keep original Book IDs and exact money in any supported migration. An incompatible runtime shows “This Vimbai build cannot open this data version.” A failed startup shows the child exit reason with a retry and a backup recovery entry point. Keep the current local Book readable during a hosted outage.

## Acceptance tests

- `fresh_windows_user_can_open_a_book_and_post_offline` launches the extracted x64 artifact on a clean Windows machine with networking disabled.
- `desktop_restart_keeps_postings_and_reuses_no_stale_runtime_origin` exercises teardown, restart and ephemeral ports.
- `windows_upgrade_preserves_book_ids_amounts_and_recovery` verifies an upgrade from the last supported artifact.
- `desktop_runtime_never_listens_on_a_lan_interface` checks the actual bound address.
- Done when the ZIP is reproducibly built, the same Book produces matching financial totals before and after upgrade, and the release manifest names unsupported operations.
