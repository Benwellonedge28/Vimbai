"""Book-scoping and persistence tests for family-community-group-service (fake Neo4j harness)."""

import importlib.util
import os

import main  # noqa: F401 (bootstraps the family_community_group_service package alias)
import pytest
from family_community_group_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("fcg_root_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "user-1", "user-2"
BOOK_A, BOOK_B = "fcg-book-a", "fcg-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _make_group(headers=H1, name="Mukando", freq="monthly", amount=100.0):
    r = client.post(
        "/groups",
        params={"name": name, "contribution_frequency": freq, "contribution_amount": amount},
        headers=headers,
    )
    assert r.status_code == 200
    return r.json()


def test_health():
    assert client.get("/health").json()["service"] == "family-community-group-service"


def test_invalid_frequency_rejected():
    r = client.post(
        "/groups",
        params={"name": "Bad", "contribution_frequency": "yearly", "contribution_amount": 10},
        headers=H1,
    )
    assert r.status_code == 400


def test_group_ownership_and_book_scoping():
    g = _make_group()
    assert g["member_count"] == 0
    assert g["current_cycle"] == 1

    # caller-scoped listing: other caller sees nothing
    assert client.get("/groups", headers=H2).json() == []
    # Book-gated: same caller, other Book sees nothing
    other_book = dict(H1)
    other_book["X-Book-ID"] = BOOK_B
    assert client.get("/groups", headers=other_book).json() == []

    # cross-caller read/update paths 404
    assert client.get(f"/groups/{g['id']}", headers=H2).status_code == 404
    assert client.post(f"/groups/{g['id']}/members", params={"name": "Intruder"}, headers=H2).status_code == 404
    assert client.post(f"/groups/{g['id']}/advance-cycle", headers=H2).status_code == 404
    assert client.get(f"/groups/{g['id']}/contributions", headers=H2).json() == []
    assert client.get(f"/groups/{g['id']}/payouts", headers=H2).json() == []


def test_members_and_contributions_scoped():
    g = _make_group()
    gid = g["id"]

    m = client.post(
        f"/groups/{gid}/members",
        params={"name": "Alice", "email": "alice@t.com", "contribution_amount": 50.0},
        headers=H1,
    ).json()
    assert m["contribution_amount"] == 50.0

    # member_count recomputed for the owner
    g2 = client.get(f"/groups/{gid}", headers=H1).json()
    assert g2["member_count"] == 1

    # member list is caller-scoped: foreign caller sees none, even by same group id
    assert client.get(f"/groups/{gid}/members", headers=H2).json() == []

    # default contribution amount falls back to the group's amount
    m2 = client.post(f"/groups/{gid}/members", params={"name": "Bob"}, headers=H1).json()
    assert m2["contribution_amount"] == 100.0

    # contribution against a foreign caller's member id: 404
    assert (
        client.post(
            f"/groups/{gid}/contribute",
            params={"member_id": m["id"], "amount": 50.0},
            headers=H2,
        ).status_code
        == 404
    )

    # owner contributes; total_pool accumulates
    c = client.post(
        f"/groups/{gid}/contribute",
        params={"member_id": m["id"], "amount": 50.0, "notes": "Jan"},
        headers=H1,
    ).json()
    assert c["amount"] == 50.0
    assert c["cycle_number"] == 1

    c2 = client.post(
        f"/groups/{gid}/contribute",
        params={"member_id": m2["id"], "amount": 25.0},
        headers=H1,
    ).json()
    g3 = client.get(f"/groups/{gid}", headers=H1).json()
    assert g3["total_pool"] == 75.0

    # contributions listing scoped + cycle filter
    assert len(client.get(f"/groups/{gid}/contributions", headers=H1).json()) == 2
    assert len(client.get(f"/groups/{gid}/contributions", params={"cycle": 1}, headers=H1).json()) == 2
    assert client.get(f"/groups/{gid}/contributions", params={"cycle": 2}, headers=H1).json() == []

    # unknown member within own group: 404
    assert (
        client.post(
            f"/groups/{gid}/contribute",
            params={"member_id": "ghost", "amount": 1.0},
            headers=H1,
        ).status_code
        == 404
    )


def test_advance_cycle():
    g = _make_group()
    r = client.post(f"/groups/{g['id']}/advance-cycle", headers=H1)
    assert r.json() == {"group_id": g["id"], "current_cycle": 2}
    # persisted
    assert client.get(f"/groups/{g['id']}", headers=H1).json()["current_cycle"] == 2
    # contributions in the new cycle stamp the new cycle number
    m = client.post(f"/groups/{g['id']}/members", params={"name": "Cara"}, headers=H1).json()
    c = client.post(f"/groups/{g['id']}/contribute", params={"member_id": m["id"], "amount": 5.0}, headers=H1).json()
    assert c["cycle_number"] == 2
