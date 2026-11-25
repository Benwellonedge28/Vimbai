# Build, distribute and verify the Android pilot

Baseline `728f9a22a5371f40fce7278c31566f8a074f2884`. A test-data release can run while 04 and 05 are being built. A pilot with live records requires their acceptance evidence.

## Existing release path

`.github/workflows/ci-main.yml` runs Flutter checks and compiles a debug APK. `apps/flutter/lib/app/config.dart::AppConfig.apiUrl` is a compile-time setting and defaults to an emulator address. `apps/flutter/android/app/build.gradle.kts` uses debug signing in the release build type at the baseline. `.github/workflows/docker-publish.yml` publishes backend images. `infrastructure/compose/compose.yaml` and `infrastructure/kubernetes/` are deployment inputs. `rust/apps/api-server/tests/accounting_end_to_end_http.rs` is a backend scenario, so record separate evidence for Android and operator recovery.

## Build and deployment

Add a manually dispatched Android release workflow to the existing workflow directory with separate pilot APK and Play AAB outputs. Reuse the same Flutter SDK as core CI. Build from the chosen Git SHA, validate the API URL as HTTPS and against the deployment allowlist, and reject emulator and localhost values for hosted artifacts. Use a protected environment for the upload or distribution keystore and its passwords. Configure release signing in Gradle to stop on missing material. Record app version, build number, Git SHA, API origin and certificate fingerprint. Verify the APK signature and AAB metadata, calculate checksums, and publish the APK and manifest to an authorized pilot channel. Treat an AAB as a Play artifact and an APK as a direct installer. Verify upgrade signing compatibility before offering an update to existing direct installs.

For a pilot deployment, pin the image digests for the Go gateway, Rust API, sync and worker. Keep Postgres private. Apply migrations against a restored staging copy first and compare the trial balance after migration. Rehearse returning to the prior compatible image and define the restore procedure if a schema change blocks that rollback. Keep runtime secrets out of the app. Monitor service health, backup age, sync failures and job backlog with Book or record identifiers omitted from general logs where possible.

## Failures and migration

A bad environment value or signature stops the release before it is visible to testers. Report a required reinstall clearly when the previous test signing key differs, and offer the recovery path from 05 before removing data. Package-level changes require an upgrade test from the last tester build. Keep a copy of the last working artifact and its backend compatibility record.

## Acceptance tests

- `pilot_build_rejects_an_emulator_or_unapproved_api_origin` tests release inputs.
- `pilot_update_opens_the_existing_book` verifies installation, signing continuity and retained local state.
- `phone_books_a_month_with_an_interrupted_sync` compares expected figures after an invoice, partial receipt, offline expense, reconciliation and month close.
- `staging_schema_upgrade_and_restore_preserve_the_trial_balance` checks the hosted operational path.
- Done when a tester can install and update the signed APK, both recovery drills have receipts, and the owner has explicitly recorded which offline operations the pilot supports. Build the Windows local runtime bundle as a later independently tested distribution work package.
