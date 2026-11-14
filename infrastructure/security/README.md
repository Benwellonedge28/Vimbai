# Security, in one place

What the platform refuses to do without, where each piece lives, and how it is checked.

## Network

`infrastructure/kubernetes/base/network-policies.yaml` is the policy set: default-deny on ingress and egress, with the
three flows the estate actually needs opened by name.

| From | To | Why |
|---|---|---|
| ingress controller | `api-gateway:8081` | the only way in |
| `api-gateway` | `calculation-api:8099` | the gateway routes, the engine answers |
| `api-gateway` | `sync-server:8099` | device traffic, same door |
| `calculation-api`, `worker`, `sync-server` | `neo4j:7687` | the store |
| everyone | `otel-collector:4317` | traces leave the pod |
| `neo4j` | `neo4j-backup` job | the backup reads the store |

The policies live beside the manifests they apply to rather than here, because kustomize has to render them with the
overlay that changes the namespace. This directory is where a reviewer reads them.

## Secrets

Nothing secret is in the repository, and the manifests name secrets rather than containing them. Each binary's
requirements, as the manifests declare them:

| Secret key | Read by | What it is |
|---|---|---|
| `jwt-secret` | `api-gateway`, `sync-server` | signs and verifies caller tokens |
| `neo4j-password` | `calculation-api`, `worker`, `sync-server` | the store |
| `encryption-key` | `calculation-api` | encrypts at rest |

An operator creates `vimbai-secrets` out of band (`kubectl create secret generic vimbai-secrets --from-literal=...`) or
supplies it from a secret manager. The base kustomization deliberately does not carry a `secretGenerator` pointing at
an env file, which is how the previous layout failed: it referenced `secrets.env`, nothing had checked one in, and
`kustomize build` could not run at all.

The on-device runtime is the one place a secret is created rather than supplied: it generates a signing secret on first
boot, keeps it beside the data it signs for, and reuses it.

## Supply chain

| Check | Where | What it catches |
|---|---|---|
| `#![forbid(unsafe_code)]` | every crate, asserted by the dependency gate | memory unsafety in our own code |
| `cargo audit` | `.github/workflows/library-ci.yml` | a dependency with a published advisory |
| `cargo deny` | `deny.toml` | licence and duplicate-dependency drift |
| images pinned by tag, never `latest` in the cluster | `infrastructure/kubernetes/base/*.yaml` | a rebuild that silently changes the binary |
| the workspace locked | `Cargo.lock` checked in, `--locked` in every image build | a dependency moving under a release |
| no Python in the image | `infrastructure/docker/Dockerfile.rust` builds Rust only | the old per-service wheels |

## What is deliberately not here

There is no Vimbai Pay secret, because Vimbai Pay was never built (ADR-005). There is no payment provider credential,
because the platform records money rather than moving it. A deployment that needs either needs a new decision, not a new
secret.
