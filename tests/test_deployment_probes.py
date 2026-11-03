"""Every k8s probe must point at a route the service actually serves.

Absorption-costing-statement and npo both shipped a Deployment whose liveness
and readiness probes hit "/" while the app had no root handler. FastAPI answers
404, so the pod is killed and restarted forever and never becomes Ready - a
green test suite and a valid manifest still add up to a service that cannot run
in the cluster. This test closes that class of mistake for all 229 services.
"""

import re
from pathlib import Path

import pytest
import yaml

REPO = Path(__file__).resolve().parents[1]
MANIFEST = REPO / "k8s" / "all-services.yaml"

ROUTE_RE = re.compile(r'@app\.(?:get|post|put|delete|patch|api_route)\(\s*"([^"]+)"')


def _deployment_probe_paths():
    probes = {}
    for doc in yaml.safe_load_all(MANIFEST.read_text()):
        if not doc or doc.get("kind") != "Deployment":
            continue
        paths = set()
        containers = doc["spec"]["template"]["spec"]["containers"]
        for container in containers:
            for kind in ("livenessProbe", "readinessProbe"):
                http_get = (container.get(kind) or {}).get("httpGet")
                if http_get:
                    paths.add(http_get["path"])
        if paths:
            probes[doc["metadata"]["name"]] = paths
    return probes


def _declared_routes(service_dir: Path):
    source = (service_dir / "main.py").read_text(encoding="utf-8", errors="replace")
    routes = set(ROUTE_RE.findall(source))
    return routes | {r.rstrip("/") or "/" for r in routes}


PROBES = _deployment_probe_paths()


@pytest.mark.parametrize("service_name", sorted(PROBES))
def test_probe_path_is_served(service_name):
    service_dir = REPO / service_name
    if not (service_dir / "main.py").is_file():
        pytest.skip(f"{service_name} has no main.py to inspect")
    routes = _declared_routes(service_dir)
    for path in PROBES[service_name]:
        normalised = path.rstrip("/") or "/"
        assert normalised in routes or f"{normalised}/" in routes, (
            f"{service_name}: the deployment probes {path!r} but nothing serves it, "
            f"so the pod would stay NotReady and restart forever"
        )
