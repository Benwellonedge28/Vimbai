#!/usr/bin/env bash
# Apply an overlay, then make the images in it actually run.
#
# The manifests carry `:latest`, so `kubectl apply -k` alone changes nothing a running pod will notice: the Deployment spec is
# identical to the one already there, no rollout is started, and the cluster keeps serving the previous image until a pod happens
# to die. Both deploy workflows used to answer that with `kubectl rollout restart deployment -n vimbai api-gateway` - the gateway
# and only the gateway - so a new engine, sync or worker image shipped on every release and ran on none of them.
#
# The set of Deployments is read from the cluster after the apply rather than listed here, because a list is what went stale: a
# fourth Deployment added to the overlay rolls out because it exists, not because somebody remembered this file.
#
#   scripts/deploy.sh production
#   scripts/deploy.sh staging
set -euo pipefail

overlay="${1:-}"
if [ -z "${overlay}" ]; then
  echo "usage: scripts/deploy.sh <overlay name under infrastructure/kubernetes/overlays>" >&2
  exit 2
fi

tree="infrastructure/kubernetes/overlays/${overlay}"
if [ ! -d "${tree}" ]; then
  echo "no overlay at ${tree}" >&2
  exit 1
fi
namespace="${VIMBAI_NAMESPACE:-vimbai}"

echo "== applying ${tree}"
kubectl apply -k "${tree}"

deployments=$(kubectl get deployment -n "${namespace}" -o jsonpath='{.items[*].metadata.name}')
if [ -z "${deployments}" ]; then
  echo "no Deployment in namespace ${namespace}, so this release would roll out nothing" >&2
  exit 1
fi

echo "== restarting: ${deployments}"
for deployment in ${deployments}; do
  kubectl rollout restart "deployment/${deployment}" -n "${namespace}"
done

# every one of them, not the first that answers: a release that leaves half its pods on the previous image is the state this
# script exists to prevent
unready=""
for deployment in ${deployments}; do
  if ! kubectl rollout status "deployment/${deployment}" -n "${namespace}" --timeout=300s; then
    unready="${unready} ${deployment}"
  fi
done

if [ -n "${unready}" ]; then
  echo "these deployments did not become ready:${unready}" >&2
  kubectl get pods -n "${namespace}" -o wide >&2 || true
  exit 1
fi

echo "== ${namespace} is running the release"
