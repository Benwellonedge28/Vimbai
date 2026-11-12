# Infrastructure

Everything that runs the platform, out of the repository root and in one place.

```
kubernetes/          manifests applied by cd-staging and cd-production
kubernetes/helm/     two charts; see below
compose/             the local stack (was docker-compose.yml at the root)
monitoring/          Prometheus, Grafana, the OTEL collector, alert rules
```

## The two Helm charts

`kubernetes/helm/vimbai/` is the live chart. It is what `ci.yml` installs and lints, and it carries
the per-environment values files.

`kubernetes/helm/legacy-chart/` was `charts/vimbai/` at the root: a smaller chart with a single
`values.yaml`, never linted or deployed by any workflow. It is kept rather than deleted because the
decision to retire it is yours, not a tidy-up's, but it is no longer at the root pretending to be
current.

## Where things moved from

`k8s/` to `kubernetes/`, `charts/vimbai/` to `kubernetes/helm/legacy-chart/`, `monitoring/` to
`monitoring/`, `docker-compose.yml` to `compose/compose.yaml`. The workflows, the four scripts that
read these paths, and the tests that pin them were re-pointed in the same commit. The kustomization
is unchanged inside its own directory, so `kubectl apply -k infrastructure/kubernetes/` behaves
exactly as it did.
