# Release one reproducible web and self-hosted deployment

Source baseline `2df3a7685b1109756fad0cca4d7b648b25790bdb`. Requires release provenance from 06 and operational evidence from 16 and 19.

## Existing deployment split

`infrastructure/compose/compose.yaml` configures Neo4j for engine, worker and sync. The engine’s deployment selection in `rust/crates/adapters/database/src/choose.rs` asks for `VIMBAI_POSTGRES_URL`. The Kubernetes calculation API and worker manifests declare that variable. `infrastructure/docker/Dockerfile.flutter-web` compiles the app without a deployed API origin, while Flutter’s `apps/flutter/lib/app/config.dart` uses the emulator default. The existing image workflow is `.github/workflows/docker-publish.yml` and the deploy entry point is `scripts/deploy.sh`.

## Release and configuration

Choose Compose as the reference self-hosted profile for a single host and keep the existing Kubernetes path for managed operators. Publish a manifest that pins the gateway, API, worker, sync and web image digests, release SHA, schema range and operating instructions. Align Compose with ADR-007. API, worker and sync receive the same supported Postgres deployment store and independent role-appropriate credentials. For existing installations, supply the migration plan from 19. Separate development fixtures and production defaults and reject placeholder secrets before startup. Define supported upgrade paths and provision secrets outside the release bundle.

Expose the web UI and gateway on one HTTPS origin. Use an explicit web API path prefix and direct it to the gateway while preserving endpoint paths. Ship the web Docker image with that origin strategy, and verify direct refreshes on app routes are served by Flutter while API paths reach the gateway. Keep the engine, worker, sync internals and database private except for the separately configured client sync route. Check API, web and sync readiness independently. Derive worker health from successful polling and job progress. Provide a privacy-conscious operator status page that shows backup age and error categories without Book contents.

## Upgrade, recovery and errors

Run database migrations as a named release step. Record the resulting schema level and reject incompatible versions before serving traffic. Take and rehearse restoring Postgres backups, and retain a release manifest that identifies the last compatible images. A startup failure names the missing setting or unavailable dependency without showing its value. A failed upgrade keeps the previous accepted release available where compatible and follows the store cutover rollback procedure where needed.

## Acceptance tests

- `signed_manifest_installs_on_a_clean_self_hosted_machine` runs the documented Docker Compose path.
- `web_login_and_a_book_read_work_on_the_published_https_origin` checks routing, restart and deep links.
- `runtime_components_agree_on_the_deployment_store` verifies a Book written through the API is read by sync and visible to the worker.
- `backup_restore_and_rollback_preserve_the_verified_book` runs before any live customer deployment.
- Done when an independent operator can install, verify, update and recover a deployment using only the release artifacts and documented runbook.
