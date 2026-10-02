"""Book-scoping and persistence tests for automation-engine-service (fake Neo4j harness).

Covers: rule CRUD persistence, toggle/delete semantics, workflow
execution with dependency validation, ownership and Book isolation.
"""

import importlib.util
import os

import main
from automation_engine_service.database import Neo4jConnector
from fastapi.testclient import TestClient

from tests.conftest import fake_module, fake_session

app = main.app

Neo4jConnector.get_driver = classmethod(lambda cls: fake_module.FakeDriver(fake_session))


client = TestClient(app)


U1, U2 = "ae-user-1", "ae-user-2"
BOOK_A, BOOK_B = "ae-book-a", "ae-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _rule(name="Daily sync", company="co-ae", **kw):
    body = {
        "name": name,
        "company_id": company,
        "trigger": "scheduled",
        "condition": {"hour": 2},
        "steps": [
            {"step_id": "s1", "step_name": "Fetch", "action": "http", "params": {"url": "https://x"}, "depends_on": []},
            {
                "step_id": "s2",
                "step_name": "Post",
                "action": "http",
                "params": {"url": "https://y"},
                "depends_on": ["s1"],
            },
        ],
        "enabled": True,
        "priority": 5,
    }
    body.update(kw)
    return body


def test_rule_crud_persists():
    r = client.post("/rules", json=_rule(), headers=H1)
    assert r.status_code == 200, r.text
    rule = r.json()
    assert rule["trigger"] == "scheduled"
    assert len(rule["steps"]) == 2

    listed = client.get("/rules", headers=H1).json()
    assert len(listed) == 1
    assert listed[0]["id"] == rule["id"]
    # nested steps round-trip intact
    assert listed[0]["steps"][1]["depends_on"] == ["s1"]
    # company filter
    assert client.get("/rules", params={"company_id": "co-ae"}, headers=H1).json()[0]["id"] == rule["id"]
    assert client.get("/rules", params={"company_id": "other"}, headers=H1).json() == []
    # single fetch
    assert client.get(f"/rules/{rule['id']}", headers=H1).json()["name"] == "Daily sync"
    # other user sees nothing
    assert client.get("/rules", headers=H2).json() == []


def test_toggle_and_delete_semantics():
    rule = client.post("/rules", json=_rule(), headers=H1).json()
    rid = rule["id"]

    # toggle flips enabled both ways
    r = client.post(f"/rules/{rid}/toggle", headers=H1)
    assert r.json() == {"rule_id": rid, "enabled": False}
    r = client.post(f"/rules/{rid}/toggle", headers=H1)
    assert r.json() == {"rule_id": rid, "enabled": True}

    # cross-scope toggle 404, original enabled state untouched
    assert client.post(f"/rules/{rid}/toggle", headers=H2).status_code == 404
    assert client.post(f"/rules/{rid}/toggle", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).status_code == 404
    assert client.get(f"/rules/{rid}", headers=H1).json()["enabled"] is True

    # delete returns False for unknown/invisible, True for own
    assert client.delete(f"/rules/{rid}", headers=H2).json() == {"deleted": False, "rule_id": rid}
    assert client.delete(f"/rules/{rid}", headers=H1).json() == {"deleted": True, "rule_id": rid}
    assert client.get("/rules", headers=H1).json() == []
    assert client.delete(f"/rules/{rid}", headers=H1).json()["deleted"] is False


def test_execute_workflow_completes():
    rule = client.post("/rules", json=_rule(), headers=H1).json()
    r = client.post(f"/execute/{rule['id']}", headers=H1)
    assert r.status_code == 200, r.text
    execution = r.json()
    assert execution["status"] == "running"
    assert execution["rule_id"] == rule["id"]
    assert execution["company_id"] == "co-ae"

    # background task ran (TestClient executes it synchronously)
    done = client.get(f"/executions/{execution['id']}", headers=H1).json()
    assert done["status"] == "completed"
    assert len(done["step_results"]) == 2
    assert done["step_results"][0]["step_id"] == "s1"
    assert done["completed_at"]

    # listing by company + status
    by_co = client.get("/executions", params={"company_id": "co-ae"}, headers=H1).json()
    assert len(by_co) == 1
    by_status = client.get("/executions", params={"status": "completed"}, headers=H1).json()
    assert len(by_status) == 1
    assert client.get("/executions", params={"status": "running"}, headers=H1).json() == []
    # other user sees nothing
    assert client.get("/executions", headers=H2).json() == []


def test_execute_dependency_failure_persisted():
    bad = _rule(name="Bad deps")
    bad["steps"][1]["depends_on"] = ["nonexistent"]
    rule = client.post("/rules", json=bad, headers=H1).json()
    execution = client.post(f"/execute/{rule['id']}", headers=H1).json()
    done = client.get(f"/executions/{execution['id']}", headers=H1).json()
    assert done["status"] == "failed"
    assert "Dependency nonexistent not completed" in done["error"]
    assert len(done["step_results"]) == 1


def test_execute_scoping_and_disabled():
    rule = client.post("/rules", json=_rule(), headers=H1).json()
    rid = rule["id"]
    # cross-scope execute 404
    assert client.post(f"/execute/{rid}", headers=H2).status_code == 404
    assert client.post(f"/execute/{rid}", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).status_code == 404
    # disabled rule -> 400
    client.post(f"/rules/{rid}/toggle", headers=H1)
    assert client.post(f"/execute/{rid}", headers=H1).status_code == 400


def test_book_a_b_isolation():
    hb = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    ra = client.post("/rules", json=_rule(name="Book A rule"), headers=H1).json()
    rb = client.post("/rules", json=_rule(name="Book B rule"), headers=hb).json()
    a = [r["name"] for r in client.get("/rules", headers=H1).json()]
    b = [r["name"] for r in client.get("/rules", headers=hb).json()]
    assert a == ["Book A rule"]
    assert b == ["Book B rule"]
    assert client.get(f"/rules/{rb['id']}", headers=H1).status_code == 404


def test_health_and_auth():
    r = client.get("/health")
    assert r.status_code == 200
    assert r.json()["status"] == "healthy"
    # authenticated health includes caller-scoped counts
    r = client.get("/health", headers=H1)
    assert r.json()["rules"] == 0
    client.post("/rules", json=_rule(), headers=H1)
    assert client.get("/health", headers=H1).json()["rules"] == 1
    # endpoints require auth
    assert client.get("/rules").status_code in (401, 403, 422)
    assert client.post("/rules", json=_rule()).status_code in (401, 403, 422)
    assert client.get("/executions").status_code in (401, 403, 422)
