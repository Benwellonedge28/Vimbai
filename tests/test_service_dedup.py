"""
Deduplication regression tests.

Two clone families were de-registered as duplicated services (source
directories stay in the repository; only registration was removed):

1. Risk family - investigation-service, risk-mitigation-service and
   risk-reporting-service were exact clones of risk-assessment-service
   (same models, same :RiskItem label + :OWNS_RISK edge, same endpoints).

2. Platform-template family - ai-readiness, cqrs-event-sourcing,
   data-sovereignty, disaster-recovery, edge-computing, event-streaming,
   federated-network, grpc, high-availability, infrastructure-as-code,
   multi-cloud, multi-tenant, observability, plugin-extension and
   offline-sync services were all the same generic Entity CRUD template
   ("platform capability" placeholders with zero domain logic, none
   referenced by the frontend).

These tests pin both families to their de-registered state.
"""

import json
import os
import re

import pytest

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

RISK_DUPLICATES = {
    "investigation-service",
    "risk-mitigation-service",
    "risk-reporting-service",
}

PLATFORM_TEMPLATE_DUPLICATES = {
    "ai-readiness-service",
    "cqrs-event-sourcing-service",
    "data-sovereignty-service",
    "disaster-recovery-service",
    "edge-computing-service",
    "event-streaming-service",
    "federated-network-service",
    "grpc-service",
    "high-availability-service",
    "infrastructure-as-code-service",
    "multi-cloud-service",
    "multi-tenant-service",
    "observability-service",
    "plugin-extension-service",
    "offline-sync-service",
}

ALL_DEREGISTERED = RISK_DUPLICATES | PLATFORM_TEMPLATE_DUPLICATES


def test_gateway_has_no_duplicate_routes():
    routes = json.load(open(os.path.join(_ROOT, "api-gateway", "config", "services.json")))["services"]
    registered = {r["name"] for r in routes}
    for dup in ALL_DEREGISTERED:
        assert dup not in registered, f"{dup} must stay de-registered from the gateway"
    # the canonical services stay routed
    assert "risk-assessment-service" in registered


@pytest.mark.parametrize("dup", sorted(ALL_DEREGISTERED))
def test_duplicate_not_deployed(dup):
    for manifest in ("k8s/all-services.yaml", "k8s/new-services.yaml", "k8s/new-services-v2.yaml"):
        path = os.path.join(_ROOT, manifest)
        if not os.path.isfile(path):
            continue
        content = open(path).read()
        assert not re.search(
            r"^  name: %s$" % re.escape(dup), content, re.M
        ), f"{dup} must not be deployed via {manifest}"


@pytest.mark.parametrize("dup", sorted(ALL_DEREGISTERED))
def test_duplicate_not_in_ci_brackets(dup):
    ci = open(os.path.join(_ROOT, ".github", "workflows", "test-core.yml")).read()
    assert not re.search(r"\b" + re.escape(dup) + r"\b", ci), f"{dup} must stay out of CI bracket lists"


@pytest.mark.parametrize("dup", sorted(ALL_DEREGISTERED))
def test_duplicate_source_dir_still_exists(dup):
    # standing rule: no repository files are ever deleted
    assert os.path.isdir(os.path.join(_ROOT, dup))
    assert os.path.isfile(os.path.join(_ROOT, dup, "main.py"))
