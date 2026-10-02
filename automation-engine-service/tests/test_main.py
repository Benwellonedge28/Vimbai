"""
Vimbai Automation Engine Service - Test Suite
Tests: health checks, rule CRUD, rule execution
"""

import importlib.util
import os
from unittest.mock import AsyncMock, patch

import pytest
from fastapi.testclient import TestClient

os.environ["JWT_SECRET"] = "test-secret-key-for-testing-only"
os.environ["NEO4J_PASSWORD"] = "test-password"

import main
from main import app

# Fake Neo4j harness (see test_book_scoping.py for the deep Book-scoping suite)
_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ae_fake_main", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
_fake_session = _fake_mod.FakeSession()

from automation_engine_service.database import Neo4jConnector

Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)

H = {"X-User-Id": "ae-main-user", "X-Book-ID": "ae-main-book"}


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


def _rule_body():
    return {
        "name": "Auto-reconcile",
        "company_id": "comp-1",
        "trigger": "scheduled",
        "steps": [{"step_id": "s1", "step_name": "Fetch", "action": "GET /transactions", "params": {}}],
    }


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200

    def test_health_endpoint(self):
        response = client.get("/health")
        assert response.status_code == 200
        assert response.json()["status"] == "healthy"


class TestRuleExecution:
    def test_rules_require_auth(self):
        assert client.post("/rules", json=_rule_body()).status_code in [401, 403, 422]
        assert client.get("/rules").status_code in [401, 403, 422]

    def test_create_and_list_rule(self):
        create = client.post("/rules", json=_rule_body(), headers=H)
        assert create.status_code == 200, create.text
        rule_id = create.json()["id"]
        assert rule_id

        listed = client.get("/rules", params={"company_id": "comp-1"}, headers=H)
        assert len(listed.json()) >= 1

    def test_execute_rule(self):
        create = client.post("/rules", json=_rule_body(), headers=H)
        rule_id = create.json()["id"]
        resp = client.post(f"/execute/{rule_id}", headers=H)
        assert resp.status_code == 200
        assert resp.json()["status"] in ("running", "completed")
