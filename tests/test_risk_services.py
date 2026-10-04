"""
Integration tests for the canonical risk-assessment-service.

investigation-service, risk-mitigation-service and risk-reporting-service
were exact clones of this service (same models, same :RiskItem label and
:OWNS_RISK edge) and have been de-registered as duplicates: their gateway
routes, k8s deployments and CI bracket memberships were removed while their
source directories stay in the repository. The dedup test below asserts they
stay de-registered.
"""

import importlib
import importlib.util
import json
import os
import re
from types import ModuleType

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_H = {"X-User-Id": "root-risk-user"}

# Exact clones of risk-assessment-service, de-registered (files kept).
DEREGISTERED_DUPLICATES = {
    "investigation-service",
    "risk-mitigation-service",
    "risk-reporting-service",
}


def _patch_fake(pkg_name, fake_alias):
    """Give the service's Neo4j connector a fake in-memory driver.

    conftest.load_service replaces the self-bootstrapped package alias with
    a plain (path-less) ModuleType, so rebuild a real package alias before
    importing the database module.
    """
    fake_path = os.path.join(_ROOT, pkg_name.replace("_", "-"), "fake_neo4j.py")
    spec = importlib.util.spec_from_file_location(fake_alias, fake_path)
    fake = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fake)

    import sys

    cached = sys.modules.get(pkg_name)
    if cached is None or not hasattr(cached, "__path__"):
        pkg_mod = ModuleType(pkg_name)
        pkg_mod.__path__ = [os.path.join(_ROOT, pkg_name.replace("_", "-"))]
        sys.modules[pkg_name] = pkg_mod

    db = importlib.import_module(f"{pkg_name}.database")
    session = fake.FakeSession()
    db.Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(session))
    return session


@pytest.fixture
def risk_client():
    pkg = load_service("risk-assessment-service")
    app = pkg.main.app
    _patch_fake("risk_assessment_service", "risk_assessment_root_fake")
    return TestClient(app)


class TestRiskAssessment:
    def test_health(self, risk_client):
        resp = risk_client.get("/")
        assert resp.status_code == 200
        assert resp.json()["service"] == "risk-assessment-service"

    def test_create_risk(self, risk_client):
        resp = risk_client.post(
            "/risks",
            json={
                "company_id": "comp-1",
                "category": "financial",
                "name": "Currency Risk",
                "description": "FX exposure risk",
                "likelihood": 4,
                "impact": 3,
            },
            headers=_H,
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["risk_score"] == 12  # 4 * 3
        assert data["level"] == "high"

    def test_create_critical_risk(self, risk_client):
        resp = risk_client.post(
            "/risks",
            json={"company_id": "comp-2", "category": "cyber", "name": "Breach", "likelihood": 5, "impact": 5},
            headers=_H,
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["risk_score"] == 25
        assert data["level"] == "critical"

    def test_get_risks_filter(self, risk_client):
        risk_client.post(
            "/risks",
            json={"company_id": "comp-f", "category": "financial", "name": "R1", "likelihood": 2, "impact": 2},
            headers=_H,
        )
        risk_client.post(
            "/risks",
            json={"company_id": "comp-f", "category": "cyber", "name": "R2", "likelihood": 5, "impact": 5},
            headers=_H,
        )
        resp = risk_client.get("/risks/comp-f", params={"category": "financial"}, headers=_H)
        assert resp.status_code == 200
        assert resp.json()["total"] == 1
        assert resp.json()["risks"][0]["name"] == "R1"

    def test_update_risk(self, risk_client):
        create = risk_client.post(
            "/risks",
            json={"company_id": "comp-up", "category": "compliance", "name": "R3", "likelihood": 2, "impact": 2},
            headers=_H,
        )
        risk_id = create.json()["id"]
        resp = risk_client.put(f"/risks/{risk_id}?likelihood=5&impact=5&mitigation=Fixed&status=mitigating", headers=_H)
        assert resp.status_code == 200
        data = resp.json()
        assert data["risk_score"] == 25
        assert data["level"] == "critical"

    def test_risk_dashboard(self, risk_client):
        risk_client.post(
            "/risks",
            json={"company_id": "comp-dash", "category": "financial", "name": "R1", "likelihood": 3, "impact": 4},
            headers=_H,
        )
        risk_client.post(
            "/risks",
            json={"company_id": "comp-dash", "category": "cyber", "name": "R2", "likelihood": 5, "impact": 5},
            headers=_H,
        )
        resp = risk_client.get("/dashboard/comp-dash", headers=_H)
        assert resp.status_code == 200
        data = resp.json()
        assert data["total_risks"] == 2
        assert len(data["top_risks"]) == 2
        assert "by_level" in data

    def test_close_risk(self, risk_client):
        create = risk_client.post(
            "/risks",
            json={"company_id": "comp-close", "category": "market", "name": "R", "likelihood": 1, "impact": 1},
            headers=_H,
        )
        risk_id = create.json()["id"]
        resp = risk_client.delete(f"/risks/{risk_id}", headers=_H)
        assert resp.status_code == 200
        assert resp.json()["status"] == "closed"


class TestRiskServiceDeduplication:
    def test_duplicate_services_are_not_routed(self):
        routes = json.load(open(os.path.join(_ROOT, "api-gateway", "config", "services.json")))["services"]
        registered = {r["name"] for r in routes}
        assert "risk-assessment-service" in registered
        for dup in DEREGISTERED_DUPLICATES:
            assert dup not in registered, f"{dup} must stay de-registered as a duplicate of risk-assessment-service"

    def test_duplicate_services_are_not_deployed(self):
        for manifest in ("k8s/all-services.yaml", "k8s/new-services.yaml", "k8s/new-services-v2.yaml"):
            path = os.path.join(_ROOT, manifest)
            if not os.path.isfile(path):
                continue
            for dup in DEREGISTERED_DUPLICATES:
                assert f"name: {dup}" not in open(path).read(), f"{dup} must not be deployed via {manifest}"

    def test_duplicate_services_are_not_in_ci_brackets(self):
        ci = open(os.path.join(_ROOT, ".github", "workflows", "test-core.yml")).read()
        for dup in DEREGISTERED_DUPLICATES:
            assert not re.search(r"\b" + re.escape(dup) + r"\b", ci), f"{dup} must stay out of CI bracket lists"
