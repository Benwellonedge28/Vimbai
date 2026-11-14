# Infrastructure

Everything that runs the platform, in one place, in one shape per environment.

```
docker/       one Rust build for three binaries, plus the gateway and the Flutter web app
compose/      the local estate: base, and dev / test / observability on top of it
kubernetes/   base plus overlays for development, staging and production
security/     the network, secret and supply-chain story, with the checks behind it
monitoring/   Prometheus, Grafana, the OTEL collector, alert rules
```

## One estate, not forty

The platform is one engine that answers every address, one gateway in front of it, one worker for the schedules and one
sync server for devices, over one database. The infrastructure says that: five containers in compose, five workloads in
the cluster. Every other service that used to have its own container, its own port and its own deployment now answers
from the engine, so its manifest would be a pod behind an address nothing routes.

## Compose

The base file is the estate; the variants add to it rather than repeating it, so a change lands once.

```
# the platform, on this machine
docker compose -f infrastructure/compose/compose.yaml up --build

# with the ports open, the store on disk and a database you can reach
docker compose -f infrastructure/compose/compose.yaml \
               -f infrastructure/compose/compose.dev.yaml up --build

# a throwaway run: no database, nothing published
docker compose -f infrastructure/compose/compose.yaml \
               -f infrastructure/compose/compose.test.yaml up --abort-on-container-exit

# and the collector it already sends traces to
docker compose -f infrastructure/compose/compose.yaml \
               -f infrastructure/compose/compose.observability.yaml up --build
```

## The cluster

`kubectl apply -k infrastructure/kubernetes/overlays/production`. Three overlays: development (one replica, local images),
staging (one replica, debug logging), production (three, and the backup schedule). The base carries what every
environment has.

There is one deployment story, and it is kustomize. The Helm chart that lived here was a second copy of the same
manifests - and the copy that had drifted: its values still listed forty services and its templates still ranged over
them, so `helm install` would have deployed pods for services that were retired months ago. Two mechanisms that disagree
is how a cluster ends up running something nobody wrote. The chart is gone; the render-it-and-check-it job in
`iac-validate.yml` renders base and every overlay instead.

## Images

| Image | Built from | What it is |
|---|---|---|
| `vimbai/engine` | `docker/Dockerfile.rust` (`PACKAGE=vimbai_api`) | the engine |
| `vimbai/worker` | `docker/Dockerfile.rust` (`PACKAGE=vimbai_worker`) | the schedules |
| `vimbai/sync` | `docker/Dockerfile.rust` (`PACKAGE=vimbai_sync`) | the sync server |
| `vimbai/gateway` | `docker/Dockerfile.gateway` | the Go gateway |
| `vimbai/flutter-web` | `docker/Dockerfile.flutter-web` | the app, for the web |

One Dockerfile for the three Rust binaries on purpose: they are one workspace, so three files would be three copies of
the same build that drift the first time a dependency changes.

## Where things moved from

`k8s/` to `kubernetes/`, `charts/vimbai/` to nothing, `docker-compose.yml` to `compose/compose.yaml`, the per-app
Dockerfiles to `docker/`. The workflows that read these paths were re-pointed in the same commit, and the repository gate
now fails if a manifest, a compose service or a chart names something that does not exist.
