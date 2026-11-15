# The pipelines

There are fourteen workflows, and each one is here for a reason that can be stated in a sentence. This page used to describe a Python
matrix over 300+ microservices, `ruff` and `bandit`, Codecov, Helm and Terraform. None of that is true any more: the services
are Rust (ADR-001), the deployment story is kustomize, and there is no Terraform to format. A stale index is worse than no
index, because it makes work look gated when it is not - so this page is checked. `workflows::documented_workflows` fails the
build when a workflow file is not named here, or when this page names one that no longer exists.

## The gates a change has to pass

| Workflow | Starts on | What it decides |
| --- | --- | --- |
| `library-ci.yml` | a change under `rust/` | The Rust workspace: no `unsafe` anywhere, `cargo fmt --check`, `clippy -D warnings`, then `cargo test --workspace` with a `postgres:16` service so the deployment-store tests answer instead of skipping. This is the gate every roadmap phase landed through. |
| `ci-main.yml` | a change to Python or Go sources, a `Dockerfile`, `gateway/go/**`, `shared/**`, `apps/flutter/**` | The other two languages: `go vet` and `go test` for a changed Go package, and `test-flutter` for the app (`dart format --set-exit-if-changed`, `flutter analyze`, `flutter test`) on the pinned toolchain. |
| `iac-validate.yml` | a change under `infrastructure/` | That the deployment actually renders: `kubectl kustomize` over the base and every overlay, and `docker compose config` for the base plus each variant. |
| `observability.yml` | a change to the monitoring configuration | That the Prometheus and collector configuration parses. |

## Scheduled and manual

| Workflow | Starts on | What it does |
| --- | --- | --- |
| `security-scan.yml` | weekly, and by hand | Secret scan, dependency scan, SBOM. |
| `registry-cleanup.yml` | schedule, and by hand | Prunes old images from the registry. |
| `repo-management.yml` | schedule, and on a pull request | Stale-issue sweep, and pull-request validation. |
| `cd-rollback.yml` | by hand only | Rolls a deployment back to a named version. |

## Releases and images

| Workflow | Starts on | What it does |
| --- | --- | --- |
| `release-automation.yml` | a push to `main` | Creates the release for the version in `VERSION`. |
| `cd-staging.yml` | a push to `develop` | Builds and deploys to staging. |
| `cd-production.yml` | a `v*.*.*` tag | Builds and deploys to production. |
| `docker-publish.yml` | a changed `Dockerfile` or dependency manifest, and by hand | Publishes the changed images. |
| `calculation-api.yml` | a change under `rust/`, and by hand | Publishes the engine image. |

## The legacy all-in-one, and why it is still here

`ci.yml` predates the split above and repeats parts of it: CodeQL, a Go build and test, a Docker build, a kustomize deploy,
and the security scans (`zap-scan`, `trivy-scan`, `grype-scan`, `dependency-vulnerabilities`, `sbom`). Its Go test step is the
only copy of the gateway's test in a pipeline that always runs it, and the scans are what the platform's security story is
built on, so the workflow is not deleted while those are unique to it. What is *not* acceptable is a duplicated step that can
fail to fail: its `go test` no longer hides behind `|| echo`, and `workflows::swallowed_test_failures` reports any workflow
that runs a test suite and then discards the verdict.

## Secrets

`KUBECONFIG` (or `KUBE_CONFIG_STAGING` and `KUBE_CONFIG_PRODUCTION`) for the deploy steps, and `GITHUB_TOKEN` for image pushes
and releases. The deploy step in `ci.yml` checks for the kubeconfig and skips, saying so, rather than failing a change that
could not be deployed from a fork.
