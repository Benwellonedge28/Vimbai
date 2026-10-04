"""Book-scoping, persistence and behaviour tests for zero-trust-data-service (fake Neo4j harness).

Covers: policy CRUD, the /evaluate decision logic (role, clearance, MFA,
IP whitelist, no-policy path), attempt persistence and tail-limit
listings, subject-user filtering, ownership and Book isolation, and
cross-scope 404s. AccessAttempt.user_id (the subject) is stored as
subject_user_id to stay distinct from the caller-ownership stamp.
"""

import importlib.util
import os

import main  # noqa: F401  (isort: keep bare main import first)
import pytest
from fastapi.testclient import TestClient

from zero_trust_data_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ztd_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "ztd-user-1", "ztd-user-2"
BOOK_A, BOOK_B = "ztd-book-a", "ztd-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _policy(resource="ledger/reports", headers=H1, **extra):
    body = {
        "resource": resource,
        "required_roles": ["accountant"],
        "required_clearance": "confidential",
        "mfa_required": True,
        "ip_whitelist": ["10.0.0.1"],
    }
    body.update(extra)
    r = client.post("/policies", json=body, headers=headers)
    assert r.status_code == 200, r.text
    return r.json()


def _evaluate(headers=H1, **extra):
    body = {
        "user_id": "subject-7",
        "resource": "ledger/reports",
        "user_roles": ["accountant"],
        "user_clearance": "confidential",
        "mfa_verified": True,
        "source_ip": "10.0.0.1",
    }
    body.update(extra)
    r = client.post("/evaluate", json=body, headers=headers)
    assert r.status_code == 200, r.text
    return r.json()


def test_policy_crud_and_persistence():
    p = _policy()
    pid = p["id"]
    assert p["required_roles"] == ["accountant"]

    assert len(client.get("/policies", headers=H1).json()) == 1

    # update replaces, keeps id
    body = {"resource": "ledger/reports", "required_roles": ["auditor"], "required_clearance": "restricted"}
    r = client.put(f"/policies/{pid}", json=body, headers=H1)
    assert r.status_code == 200
    assert r.json()["id"] == pid
    assert r.json()["required_roles"] == ["auditor"]

    # delete removes; second delete 404
    assert client.delete(f"/policies/{pid}", headers=H1).json()["deleted"] is True
    assert client.delete(f"/policies/{pid}", headers=H1).status_code == 404
    assert client.get("/policies", headers=H1).json() == []


def test_evaluate_decision_logic():
    _policy()

    # fully compliant request is granted
    a = _evaluate()
    assert a["granted"] is True
    assert a["reason"] == "Access granted"
    assert a["policy_id"] != ""

    # missing role
    a = _evaluate(user_roles=["viewer"])
    assert a["granted"] is False
    assert "Missing required role" in a["reason"]

    # insufficient clearance
    a = _evaluate(user_clearance="internal")
    assert a["granted"] is False
    assert "Insufficient clearance" in a["reason"]

    # MFA not verified
    a = _evaluate(mfa_verified=False)
    assert a["granted"] is False
    assert "MFA required but not verified" in a["reason"]

    # IP not whitelisted
    a = _evaluate(source_ip="203.0.113.9")
    assert a["granted"] is False
    assert "IP not in whitelist" in a["reason"]

    # no policy for the resource
    a = _evaluate(resource="other/resource")
    assert a["granted"] is False
    assert a["reason"] == "No policy found for resource"
    assert a["policy_id"] == ""


def test_attempts_persisted_and_tail_limit():
    _policy()
    for i in range(5):
        _evaluate(user_id=f"subject-{i}")

    attempts = client.get("/attempts", headers=H1).json()
    assert len(attempts) == 5
    # subject user ids survive the prop aliasing round-trip
    assert {a["user_id"] for a in attempts} == {f"subject-{i}" for i in range(5)}

    # tail limit keeps the most recent
    limited = client.get("/attempts", params={"limit": 2}, headers=H1).json()
    assert len(limited) == 2
    assert {a["user_id"] for a in limited} == {"subject-3", "subject-4"}

    # subject filter
    user_a = client.get("/attempts/user/subject-2", headers=H1).json()
    assert len(user_a) == 1
    assert user_a[0]["user_id"] == "subject-2"


def test_cross_user_and_book_isolation():
    p1 = _policy(resource="res/one")
    p2 = _policy(resource="res/two", headers=H2)
    pb = _policy(resource="res/three", headers=HB)

    # policies visible only to their owner/Book
    assert len(client.get("/policies", headers=H1).json()) == 1
    assert len(client.get("/policies", headers=H2).json()) == 1
    assert len(client.get("/policies", headers=HB).json()) == 1
    assert client.get("/policies", headers=H1).json()[0]["id"] == p1["id"]

    # cross-scope policy ops 404
    assert client.put(f"/policies/{p1['id']}", json={"resource": "x"}, headers=H2).status_code == 404
    assert client.delete(f"/policies/{p2['id']}", headers=H1).status_code == 404
    assert client.delete(f"/policies/{pb['id']}", headers=H1).status_code == 404

    # evaluate consults only the caller's policies
    a = _evaluate(headers=H2, resource="res/one")  # U2 has no policy for res/one
    assert a["granted"] is False
    assert a["reason"] == "No policy found for resource"

    # attempts recorded for the caller only (U2 has their earlier no-policy attempt)
    _evaluate()
    assert len(client.get("/attempts", headers=H1).json()) == 1
    assert len(client.get("/attempts", headers=H2).json()) == 1
