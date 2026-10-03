"""Book-scoping and persistence tests for admin-service (fake Neo4j harness)."""

import importlib.util
import os

import main  # noqa: F401 (bootstraps the admin_service package alias)
import pytest
from admin_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("admin_root_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "admin-1", "admin-2"
BOOK_A, BOOK_B = "admin-book-a", "admin-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def test_health():
    body = client.get("/").json()
    assert body["status"] == "healthy"
    assert body["service"] == "admin"


def test_feature_override_is_per_caller():
    before = {f["id"]: f["status"] for f in client.get("/features", headers=H1).json()}
    assert before["budgeting"] == "enabled"

    # U1 disables budgeting — only U1's view changes
    r = client.post("/features/budgeting/disable", headers=H1)
    assert r.json() == {"status": "disabled", "feature_id": "budgeting"}

    u1_view = {f["id"]: f["status"] for f in client.get("/features", headers=H1).json()}
    u2_view = {f["id"]: f["status"] for f in client.get("/features", headers=H2).json()}
    assert u1_view["budgeting"] == "disabled"
    assert u2_view["budgeting"] == "enabled"  # untouched catalog for others

    # Book-gated too: same user, other Book sees the catalog default
    other_book = {
        f["id"]: f["status"] for f in client.get("/features", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()
    }
    assert other_book["budgeting"] == "enabled"

    # enable restores it
    client.post("/features/budgeting/enable", headers=H1)
    u1_view = {f["id"]: f["status"] for f in client.get("/features", headers=H1).json()}
    assert u1_view["budgeting"] == "enabled"

    # PUT override with rollout percentage + status
    r = client.put("/features/forecasting", json={"status": "enabled", "rollout_percentage": 75}, headers=H1)
    assert r.status_code == 200
    assert r.json()["status"] == "enabled"
    assert r.json()["rollout_percentage"] == 75
    # partial update preserves previously set fields
    r = client.put("/features/forecasting", json={"status": "beta"}, headers=H1)
    assert r.json()["rollout_percentage"] == 75
    assert r.json()["status"] == "beta"

    # unknown feature: 404
    assert client.put("/features/nope", json={"status": "enabled"}, headers=H1).status_code == 404


def test_dependencies_check_is_caller_scoped():
    # catalog: budgeting+scenario_modeling enabled by default, so forecasting deps satisfied
    body = client.get("/features/forecasting/dependencies", headers=H1).json()
    assert body["satisfied"] is True

    # U1 disables budgeting → U1's dependency check fails, U2's still satisfied
    client.post("/features/budgeting/disable", headers=H1)
    assert client.get("/features/forecasting/dependencies", headers=H1).json()["satisfied"] is False
    assert client.get("/features/forecasting/dependencies", headers=H2).json()["satisfied"] is True


def test_org_feature_configs_are_caller_owned():
    # U1 configures org-42's budgeting
    r = client.put(
        "/organizations/org-42/features/budgeting",
        params={"enabled": False, "notes": "hold for audit"},
        headers=H1,
    )
    assert r.status_code == 200
    cfg = r.json()
    assert cfg["enabled"] is False
    assert cfg["disabled_at"] is not None

    # U1 sees the org override merged into the org view
    org_view = {f["id"]: f for f in client.get("/organizations/org-42/features", headers=H1).json()}
    assert org_view["budgeting"]["org_enabled"] is False

    # U2 sees the catalog default for the same org (no cross-caller leak)
    u2_view = {f["id"]: f for f in client.get("/organizations/org-42/features", headers=H2).json()}
    assert u2_view["budgeting"]["org_enabled"] is True

    # re-enable: enabled_at preserved semantics (stamps enabled_at, clears disabled_at)
    cfg2 = client.post("/organizations/org-42/features/budgeting/enable", headers=H1).json()
    assert cfg2["enabled"] is True
    assert cfg2["enabled_at"] is not None
    assert cfg2["disabled_at"] is None

    # foreign feature id: 404
    assert client.put("/organizations/org-42/features/nope", params={"enabled": True}, headers=H1).status_code == 404


def test_rollout_schedules_and_feature_requests_scoped():
    # U1 schedules a rollout
    r = client.post(
        "/rollout-schedules",
        params={"feature_id": "budgeting", "scheduled_date": "2026-11-01T00:00:00Z", "target_percentage": 50},
        headers=H1,
    )
    assert r.status_code == 200
    assert len(client.get("/rollout-schedules", headers=H1).json()) == 1
    assert client.get("/rollout-schedules", headers=H2).json() == []

    # cancel by feature id (original semantics): foreign caller can't cancel
    assert client.delete("/rollout-schedules/budgeting", headers=H2).status_code == 404
    assert client.delete("/rollout-schedules/budgeting", headers=H1).json()["status"] == "cancelled"
    cancelled = client.get("/rollout-schedules", headers=H1).json()
    assert cancelled[0]["status"] == "cancelled"

    # feature requests
    r = client.post(
        "/feature-requests",
        params={"user_id": "u1", "user_email": "u1@x.com", "feature_name": "ZWL reporting"},
        headers=H1,
    )
    rid = r.json()["id"]
    assert len(client.get("/feature-requests", headers=H1).json()) == 1
    assert client.get("/feature-requests", headers=H2).json() == []

    # review is caller-scoped: foreign 404, owner ok
    assert (
        client.put(
            "/feature-requests/%s/review" % rid,
            params={"status": "approved", "reviewed_by": "rev"},
            headers=H2,
        ).status_code
        == 404
    )
    reviewed = client.put(
        "/feature-requests/%s/review" % rid,
        params={"status": "approved", "reviewed_by": "rev"},
        headers=H1,
    ).json()
    assert reviewed["status"] == "approved"
    assert reviewed["reviewed_at"] is not None

    # delete: foreign 404, owner ok
    assert client.delete("/feature-requests/%s" % rid, headers=H2).status_code == 404
    assert client.delete("/feature-requests/%s" % rid, headers=H1).json()["status"] == "deleted"
    assert client.get("/feature-requests/%s" % rid, headers=H1).status_code == 404


def test_config_overrides_and_audit_scoping():
    # default config value
    assert client.get("/config/base_currency", headers=H1).json()["value"] == "USD"

    # U1 overrides — U2 keeps default
    r = client.put("/config/base_currency", params={"value": "ZWL"}, headers=H1)
    assert r.json()["value"] == "ZWL"
    assert client.get("/config/base_currency", headers=H2).json()["value"] == "USD"
    # Book-gated: same user other Book sees default
    assert client.get("/config/base_currency", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()["value"] == "USD"

    # unknown key: 404
    assert client.put("/config/nope", params={"value": 1}, headers=H1).status_code == 404

    # audit trail: U1's actions generated caller-owned entries; U2 sees none
    u1_logs = client.get("/audit-logs", headers=H1).json()
    assert any(e["action"] == "config_updated" and e["resource_id"] == "base_currency" for e in u1_logs)
    assert client.get("/audit-logs", headers=H2).json() == []

    # manual audit entry + filter
    entry = client.post(
        "/audit-logs",
        params={
            "user_id": "auditor",
            "user_email": "auditor@x.com",
            "action": "manual_check",
            "resource_type": "config",
            "resource_id": "base_currency",
        },
        headers=H1,
    ).json()
    assert entry["action"] == "manual_check"
    assert len(client.get("/audit-logs", params={"action": "manual_check"}, headers=H1).json()) == 1
    assert client.get("/audit-logs", params={"action": "manual_check"}, headers=H2).json() == []


def test_dashboard_stats_scoped():
    # empty state for a fresh caller
    stats = client.get("/dashboard/stats", headers=H2).json()
    assert stats["audit_logs_count"] == 0
    assert stats["feature_requests"]["total"] == 0
    assert stats["organization_configs"] == 0

    client.post("/features/budgeting/disable", headers=H1)
    client.put("/organizations/org-42/features/budgeting", params={"enabled": False}, headers=H1)
    client.post(
        "/feature-requests",
        params={"user_id": "u1", "user_email": "u1@x.com", "feature_name": "ZWL reporting"},
        headers=H1,
    )

    u1 = client.get("/dashboard/stats", headers=H1).json()
    u2 = client.get("/dashboard/stats", headers=H2).json()
    assert u1["enabled_features"] < u2["enabled_features"]  # budgeting disabled for U1 only
    assert u1["feature_requests"]["pending"] == 1
    assert u2["feature_requests"]["pending"] == 0
    assert u1["organization_configs"] == 1
    assert u2["organization_configs"] == 0
    assert u1["audit_logs_count"] >= 1
    assert u2["audit_logs_count"] == 0


def test_services_health_static():
    services = client.get("/services/health", headers=H1).json()
    names = {s["service_name"] for s in services}
    assert "accounting-service" in names
    assert all(s["status"] == "healthy" for s in services)
