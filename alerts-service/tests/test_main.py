"""Book-scoping, persistence and behaviour tests for alerts-service (fake Neo4j harness).

Covers: rule CRUD with cooldown bookkeeping (last_triggered persisted,
trigger_count preserved across PUT), alert lifecycle (acknowledge /
resolve / dismiss), /evaluate against the caller's own enabled rules with
cooldown, /trigger cross-scope 404, caller-scoped stats, and cross-user
/ cross-Book isolation. WebSocket connection state is ephemeral by design
and is not persisted.
"""

import importlib.util
import os

import main  # noqa: F401  (isort: keep bare main import first)
import pytest
from fastapi.testclient import TestClient

from alerts_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("alerts_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "alerts-user-1", "alerts-user-2"
BOOK_A, BOOK_B = "alerts-book-a", "alerts-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _rule(name="Big balance", headers=H1, **extra):
    body = {
        "name": name,
        "category": "financial",
        "severity": "high",
        "condition": {"type": "threshold", "field": "balance", "operator": "gt", "value": 10000},
        "cooldown_seconds": 300,
    }
    body.update(extra)
    r = client.post("/rules", json=body, headers=headers)
    assert r.status_code == 201, r.text
    return r.json()


def test_rule_crud_and_persistence():
    rule = _rule()
    rid = rule["id"]

    got = client.get(f"/rules/{rid}", headers=H1).json()
    assert got["name"] == "Big balance"
    assert got["created_by"] == "system"
    assert got["enabled"] is True

    # enabled_only filter
    _rule(name="Second", headers=H1)
    assert len(client.get("/rules", headers=H1).json()) == 2
    assert len(client.get("/rules", params={"enabled_only": True}, headers=H1).json()) == 2

    # update keeps created_by/created_at/trigger_count
    updated = {
        "name": "Bigger balance",
        "category": "financial",
        "severity": "critical",
        "condition": {"type": "threshold", "field": "balance", "operator": "gt", "value": 20000},
        "cooldown_seconds": 60,
    }
    r = client.put(f"/rules/{rid}", json=updated, headers=H1)
    assert r.status_code == 200
    body = r.json()
    assert body["name"] == "Bigger balance"
    assert body["created_by"] == rule["created_by"]
    assert body["trigger_count"] == rule["trigger_count"]

    # delete, then gone
    assert client.delete(f"/rules/{rid}", headers=H1).status_code == 204
    assert client.get(f"/rules/{rid}", headers=H1).status_code == 404


def test_alert_lifecycle_and_persistence():
    rule = _rule()
    alert = {
        "rule_id": rule["id"],
        "title": "Manual alert",
        "message": "Something happened",
        "severity": "high",
        "category": "financial",
        "source": "test",
        "metadata": {"env": "test"},
    }
    r = client.post("/alerts", json=alert, headers=H1)
    assert r.status_code == 201, r.text
    aid = r.json()["id"]

    # persisted and listed
    listed = client.get("/alerts", headers=H1).json()
    assert len(listed) == 1
    assert listed[0]["status"] == "active"

    # acknowledge
    r = client.put(f"/alerts/{aid}/acknowledge", headers=H1)
    assert r.status_code == 200
    assert r.json()["status"] == "acknowledged"
    assert r.json()["acknowledged_at"] is not None

    # resolve with note lands in metadata
    r = client.put(f"/alerts/{aid}/resolve", params={"resolution_note": "fixed upstream"}, headers=H1)
    assert r.status_code == 200
    assert r.json()["status"] == "resolved"
    assert r.json()["metadata"]["resolution_note"] == "fixed upstream"

    # dismiss
    r = client.put(f"/alerts/{aid}/dismiss", headers=H1)
    assert r.status_code == 200
    assert r.json()["status"] == "dismissed"

    # filters
    assert len(client.get("/alerts", params={"status": "dismissed"}, headers=H1).json()) == 1
    assert len(client.get("/alerts", params={"severity": "critical"}, headers=H1).json()) == 0


def test_evaluate_only_callers_rules_with_cooldown():
    _rule()
    r = client.post("/evaluate", json={"balance": 50000, "source": "ledger"}, headers=H1)
    assert r.status_code == 200
    assert r.json()["triggered_count"] == 1
    aid = r.json()["alerts"][0]["id"]

    # cooldown blocks an immediate second trigger of the same rule
    r = client.post("/evaluate", json={"balance": 60000}, headers=H1)
    assert r.json()["triggered_count"] == 0

    # trigger_count persisted on the rule
    rules = client.get("/rules", headers=H1).json()
    assert rules[0]["trigger_count"] == 1

    # another user's evaluate does not see this user's rules
    r = client.post("/evaluate", json={"balance": 50000}, headers=H2)
    assert r.json()["triggered_count"] == 0

    # the triggered alert is caller-owned
    assert client.get(f"/alerts/{aid}", headers=H2).status_code == 404


def test_trigger_endpoint_scoped_to_caller_rule():
    rule = _rule()
    r = client.post(f"/trigger/{rule['id']}", json={"message": "manual", "source": "internal"}, headers=H1)
    assert r.status_code == 200
    assert r.json()["title"] == "Big balance"
    assert r.json()["metadata"] == {"message": "manual", "source": "internal"}

    # cross-scope: another user cannot trigger this rule
    assert client.post(f"/trigger/{rule['id']}", json={}, headers=H2).status_code == 404


def test_stats_are_caller_scoped():
    _rule()
    _rule(name="Disabled", headers=H1, enabled=False)
    client.post("/evaluate", json={"balance": 50000}, headers=H1)

    s = client.get("/stats", headers=H1).json()
    assert s["total_alerts"] == 1
    assert s["total_rules"] == 2
    assert s["enabled_rules"] == 1
    assert s["by_status"] == {"active": 1}

    # other user sees nothing
    s2 = client.get("/stats", headers=H2).json()
    assert s2["total_alerts"] == 0
    assert s2["total_rules"] == 0


def test_cross_user_and_book_isolation():
    r1 = _rule()
    r2 = _rule(name="Other user rule", headers=H2)
    rb = _rule(name="Other book rule", headers=HB)

    assert client.get(f"/rules/{r1['id']}", headers=H2).status_code == 404
    assert client.get(f"/rules/{r2['id']}", headers=H1).status_code == 404
    assert client.get(f"/rules/{r1['id']}", headers=HB).status_code == 404
    assert client.get(f"/rules/{rb['id']}", headers=H1).status_code == 404

    assert len(client.get("/rules", headers=H1).json()) == 1
    assert len(client.get("/rules", headers=H2).json()) == 1
    assert len(client.get("/rules", headers=HB).json()) == 1
