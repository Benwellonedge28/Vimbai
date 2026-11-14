"""
Deduplication guard for the risk register.

investigation-service, risk-mitigation-service and risk-reporting-service were exact clones of the risk service (same
models, same :RiskItem label and :OWNS_RISK edge) and were de-registered: their gateway routes, k8s deployments and CI
bracket memberships were removed. The tests below assert they stay out.

The register's own contract is tested against the Rust engine now - create and score, filter by category and level,
reassess with a mitigation, the dashboard and close - in rust/apps/api-server/tests/compliance_http.rs, over the same
/risk-assessment addresses the gateway publishes.
"""

import json
import os
import re

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Exact clones of the risk service, de-registered (files kept).
DEREGISTERED_DUPLICATES = {
    "investigation-service",
    "risk-mitigation-service",
    "risk-reporting-service",
}


class TestRiskServiceDeduplication:
    def test_duplicate_services_are_not_routed(self):
        routes = json.load(open(os.path.join(_ROOT, "gateway", "go", "config", "services.json")))["services"]
        registered = {r["name"] for r in routes}
        assert "risk-assessment-service" in registered
        for dup in DEREGISTERED_DUPLICATES:
            assert dup not in registered, f"{dup} must stay de-registered as a duplicate of risk-assessment-service"

    def test_duplicate_services_are_not_deployed(self):
        for manifest in (
            "infrastructure/kubernetes/all-services.yaml",
            "infrastructure/kubernetes/new-services.yaml",
            "infrastructure/kubernetes/new-services-v2.yaml",
        ):
            path = os.path.join(_ROOT, manifest)
            if not os.path.isfile(path):
                continue
            for dup in DEREGISTERED_DUPLICATES:
                assert f"name: {dup}" not in open(path).read(), f"{dup} must not be deployed via {manifest}"

    def test_duplicate_services_are_not_in_ci_brackets(self):
        ci = open(os.path.join(_ROOT, ".github", "workflows", "test-core.yml")).read()
        for dup in DEREGISTERED_DUPLICATES:
            assert not re.search(r"\b" + re.escape(dup) + r"\b", ci), f"{dup} must stay out of CI bracket lists"
