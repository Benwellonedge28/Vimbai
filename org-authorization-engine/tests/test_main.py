"""Book-scoping and persistence tests for org-authorization-engine (fake Neo4j harness)."""

import importlib.util
import os

import main  # noqa: F401 (bootstraps the org_authorization_engine package alias)
import pytest
from fastapi.testclient import TestClient
from org_authorization_engine.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("oa_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "oa-user-1", "oa-user-2"
BOOK_A, BOOK_B = "oa-book-a", "oa-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_role(name, permissions, headers=H1):
    r = client.post(
        "/roles",
        params={"name": name, "permissions": permissions},
        headers=headers,
    )
    assert r.status_code == 200, r.text
    return r.json()


def test_health():
    body = client.get("/health").json()
    assert body["status"] == "healthy"
    assert body["service"] == "org-authorization-engine"


def test_role_isolation():
    _mk_role("finance_admin", ["read:reports", "write:reports"])
    _mk_role("hr_admin", ["read:hr"], headers=H2)

    # each caller sees only own roles
    assert len(client.get("/roles", headers=H1).json()) == 1
    assert len(client.get("/roles", headers=H2).json()) == 1

    # Book-gated: same user, other Book sees nothing
    assert client.get("/roles", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json() == []


def test_assign_check_and_revocation_scoping():
    role = _mk_role("finance_admin", ["read:reports", "write:reports"])

    # assigning against a foreign role id: 404 (role invisible to U2)
    r = client.post(
        "/assign",
        params={"user_id": "user1", "org_id": "org1", "role_id": role["id"]},
        headers=H2,
    )
    assert r.status_code == 404

    # owner assigns fine
    r = client.post(
        "/assign",
        params={"user_id": "user1", "org_id": "org1", "role_id": role["id"]},
        headers=H1,
    )
    assert r.status_code == 200, r.text
    assert r.json()["role_id"] == role["id"]

    # duplicate within same caller/org/user: 409
    r = client.post(
        "/assign",
        params={"user_id": "user1", "org_id": "org1", "role_id": role["id"]},
        headers=H1,
    )
    assert r.status_code == 409

    # check resolves against caller's own RBAC config
    ok = client.post(
        "/check", json={"user_id": "user1", "org_id": "org1", "permission": "read:reports"}, headers=H1
    ).json()
    assert ok == {"allowed": True, "role": "finance_admin", "permission": "read:reports"}

    # a foreign caller's check over the same subject/org: no assignments visible
    foreign = client.post(
        "/check", json={"user_id": "user1", "org_id": "org1", "permission": "read:reports"}, headers=H2
    ).json()
    assert foreign == {"allowed": False, "reason": "No role assignments found"}

    # ungranted permission is denied
    denied = client.post(
        "/check", json={"user_id": "user1", "org_id": "org1", "permission": "delete:reports"}, headers=H1
    ).json()
    assert denied["allowed"] is False
    assert denied["reason"] == "Permission not granted by any assigned role"

    # user roles listing is caller-scoped
    assert len(client.get("/user/user1/roles", headers=H1).json()) == 1
    assert client.get("/user/user1/roles", headers=H2).json() == []
    filtered = client.get("/user/user1/roles", params={"org_id": "other-org"}, headers=H1).json()
    assert filtered == []

    # revoke: foreign 404, owner ok
    r = client.post(
        "/assign",
        params={"user_id": "user1", "org_id": "org2", "role_id": role["id"]},
        headers=H1,
    )
    aid = r.json()["id"]
    assert client.delete(f"/assign/{aid}", headers=H2).status_code == 404
    assert client.delete(f"/assign/{aid}", headers=H1).json()["revoked"] is True
    assert client.delete(f"/assign/{aid}", headers=H1).status_code == 404

    # after revoking the org1 assignment too, check denies
    aids = [a["id"] for a in client.get("/user/user1/roles", params={"org_id": "org1"}, headers=H1).json()]
    for aid in aids:
        client.delete(f"/assign/{aid}", headers=H1)
    assert client.post(
        "/check", json={"user_id": "user1", "org_id": "org1", "permission": "read:reports"}, headers=H1
    ).json() == {"allowed": False, "reason": "No role assignments found"}
