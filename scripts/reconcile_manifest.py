"""Compare what the gateway advertises, what the cluster runs, and what exists on disk.

Three retirement batches found services still wired into the cluster after they had
disappeared from the gateway manifest, and one batch found the reverse. Nothing was
checking either direction, so a retired service could quietly keep a deployment behind
it. This reports the three-way diff, and fixes the two directions that are safe to fix
automatically: a manifest entry with no directory, and a cluster document with no
manifest entry.

Usage: python3 scripts/reconcile_manifest.py [--fix]
"""

import json
import os
import pathlib
import re
import sys

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = os.path.join(ROOT, "gateway", "go", "config", "services.json")
K8S = os.path.join(ROOT, "infrastructure", "kubernetes")

# Services the cluster still runs that the gateway never advertised. Each one is a
# deliberate open question rather than drift, and the status test names them too, so
# they are allowed here instead of being quietly removed.
# Deployments that are infrastructure rather than services the gateway routes to.
INFRA = {"api-gateway", "jaeger", "otel-collector"}

# Services whose code lives in the Rust workspace (rust/) instead of a top-level
# directory. There is no <name>/main.py to find, and its absence is not drift.
IN_WORKSPACE = {"calculation-api"}

UNDECIDED = {
    "finance-service",
    "fraud-detection-service",
    "identity-service",
    "supply-chain-service",
}
# A cluster document names its service under metadata, two spaces in. Container env
# entries are also called name, which is why the metadata anchor matters.
METADATA_LINE = re.compile(r"^metadata:\s*$", re.MULTILINE)
DOC_NAME_LINE = re.compile(r"^  name: ([a-z0-9][a-z0-9-]*)\s*$", re.MULTILINE)


def manifest_names():
    with open(MANIFEST) as handle:
        return [entry["name"] for entry in json.load(handle)["services"]]


def document_name(document):
    """The workload name, from a Deployment only.

    Parsed properly rather than by regex: an autoscaler document names its target as
    scaleTargetRef, and reading a name near "metadata" picked those up as deployments.
    """
    try:
        payload = yaml.safe_load(document)
    except yaml.YAMLError:
        return None
    if not isinstance(payload, dict) or payload.get("kind") != "Deployment":
        return None
    name = (payload.get("metadata") or {}).get("name")
    return name if isinstance(name, str) else None


def deployed_manifests():
    """Only the manifests kustomize actually applies.

    vimbai-all-services.yaml is an older full-cluster dump that nothing references any
    more; reading it would report every service as drift.
    """
    kustomization = pathlib.Path(K8S, "kustomization.yaml").read_text()
    block = kustomization.split("resources:")[1].split("\n\n")[0]
    listed = re.findall(r"^- (\S+)$", block, re.MULTILINE)
    return [pathlib.Path(K8S, name) for name in listed if name.endswith(".yaml")]


def cluster_documents():
    for path in deployed_manifests():
        yield from path.read_text().split("\n---\n")


def cluster_names():
    return sorted({name for name in map(document_name, cluster_documents()) if name and name not in INFRA})


def directory_names():
    return sorted(name for name in os.listdir(ROOT) if os.path.isfile(os.path.join(ROOT, name, "main.py")))


def drift():
    manifest = manifest_names()
    cluster = cluster_names()
    deployed = set(manifest)
    return {
        "advertised without a directory": sorted(
            name for name in manifest if name not in set(directory_names()) and name not in IN_WORKSPACE
        ),
        "in the cluster but not the gateway": sorted(
            name for name in cluster if name not in deployed and name not in UNDECIDED
        ),
        "advertised but absent from the cluster": sorted(name for name in manifest if name not in set(cluster)),
    }


def fix():
    fixed = []
    manifest = manifest_names()
    with open(MANIFEST) as handle:
        payload = json.load(handle)
    kept = [entry for entry in payload["services"] if os.path.isdir(os.path.join(ROOT, entry["name"]))]
    if len(kept) != len(payload["services"]):
        payload["services"] = kept
        with open(MANIFEST, "w") as handle:
            json.dump(payload, handle, indent=2)
            handle.write("\n")
        fixed.append(f"manifest entries without a directory: {len(manifest) - len(kept)}")

    deployed = {entry["name"] for entry in kept}
    for path in deployed_manifests():
        documents = path.read_text().split("\n---\n")
        survivors, dropped = [], []
        for document in documents:
            name = document_name(document)
            if name and name not in deployed and name not in UNDECIDED and name not in INFRA:
                dropped.append(name)
            else:
                survivors.append(document)
        if dropped:
            path.write_text("\n---\n".join(survivors))
            fixed.append(
                f"{path.name}: cluster documents without a gateway entry: " f"{', '.join(sorted(set(dropped)))}"
            )
    return fixed


def main():
    report = drift()
    if "--fix" in sys.argv:
        for line in fix():
            print(line)
        report = drift()
    empty = True
    for label, names in report.items():
        print(f"{label}: {len(names)}")
        for name in names:
            empty = False
            print(f"    {name}")
    return 0 if empty else 1


if __name__ == "__main__":
    sys.exit(main())
