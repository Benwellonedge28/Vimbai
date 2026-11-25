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
| `security-scan.yml` | weekly, and by hand | Secret scan (TruffleHog), dependency scan (Trivy, advisory), `cargo audit` over the lock file the engine ships, CodeQL over the gateway, and an SBOM. |
| `registry-cleanup.yml` | schedule, and by hand | Prunes old images from the registry. |
| `repo-management.yml` | schedule, and on a pull request | Stale-issue sweep, and pull-request validation. |
| `cd-rollback.yml` | by hand only | Rolls a deployment back to a named version. |
| `documentation-import.yml` | by hand only | Takes a package of Markdown documents, writes only the ones whose paths stay inside `docs/`, and opens a pull request for review rather than writing to `main`. |

## Releases and images

| Workflow | Starts on | What it does |
| --- | --- | --- |
| `release-automation.yml` | a push to `main` | Creates the release for the version in `VERSION`. |
| `cd-staging.yml` | a push to `develop` | Builds and deploys to staging. |
| `cd-production.yml` | a `v*.*.*` tag | Builds and deploys to production. |
| `docker-publish.yml` | a change under `rust/`, `gateway/go/` or `infrastructure/docker/`, and by hand | Publishes the four images the cluster pulls (engine, sync, worker, gateway). |

## What main will not accept

`main` is protected by a ruleset, so the gates above are the only way in. Nothing deletes it, nothing force pushes over
it, and a change arrives as a pull request whose three checks - `Rust fmt / clippy / test`, `test-flutter` and `test-go` -
have reported. The repository admin role bypasses the ruleset, which is how a person or a session holding the token
still pushes directly when that is what they mean to do.

The gates therefore start on every pull request, not only on the paths they cover. They used to be path filtered, and a
documentation-only pull request then started neither workflow: its required checks stayed expected for ever and the pull
request could never merge. The jobs inside still decide what work is worth doing, so a change that touches no Rust skips
the Rust job rather than failing it.

This is the shape that lets an outside agent write to the repository. It may do anything; it may land nothing the gates
would refuse.

## What was removed, and why

`ci.yml` was a 400-line all-in-one. It is gone, and it is worth saying why rather than leaving its absence to be wondered at.
It had stopped running at all: two of its jobs, `lint-python` and `test-python-services`, were deleted when the services became
Rust, and three other jobs still listed them under `needs`, so GitHub rejected the whole file and none of its jobs - CodeQL,
Trivy, Grype, ZAP, the SBOM, the failure notifier - had executed since. What it still had that mattered was `cargo audit` and
CodeQL, both now in `security-scan.yml`. What it duplicated is covered: its Go build and test by `ci-main.yml`, its image build
by `docker-publish.yml`, its deploy by `cd-production.yml`. That last one is the reason not to restore
it: it applied the production overlay on every push to `main`, so production had a second, untagged door beside the release
workflow. Its ZAP job scanned `https://www.example.com` and its Grype job scanned the same filesystem Trivy already covers, so
neither is missed.

## Secrets

`KUBECONFIG`, `KUBE_CONFIG_STAGING` and `KUBE_CONFIG_PRODUCTION` for the two deploy workflows, and `GITHUB_TOKEN` for image
pushes and releases. The deploy steps check for the kubeconfig first and skip, saying so, rather than failing a change that
could not be deployed from a fork.
