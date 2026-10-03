"""Book-scoping and persistence tests for treasury-policy-service (fake Neo4j harness)."""

import importlib.util
import os

import main  # noqa: F401 (bootstraps the treasury_policy_service package alias)
import pytest
from fastapi.testclient import TestClient
from treasury_policy_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("tp_root_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "tp-user-1", "tp-user-2"
BOOK_A, BOOK_B = "tp-book-a", "tp-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _make_policy(headers=H1, category="fx_risk"):
    r = client.post(
        "/policies",
        params={
            "name": "FX Exposure Policy",
            "description": "Limit FX exposure",
            "policy_category": category,
            "effective_date": "2026-01-01T00:00:00",
            "approved_by": "CFO",
        },
        headers=headers,
    )
    assert r.status_code == 200
    return r.json()


def _make_limit(policy_id, headers=H1, value=1_000_000.0):
    r = client.post(
        f"/policies/{policy_id}/limits",
        params={"limit_type": "exposure", "limit_value": value, "currency": "USD", "warning_threshold": 0.8},
        headers=headers,
    )
    assert r.status_code == 200
    return r.json()


def test_health():
    assert client.get("/health").json()["service"] == "treasury-policy-service"


def test_invalid_category():
    r = client.post(
        "/policies",
        params={
            "name": "Bad",
            "description": "x",
            "policy_category": "crypto",
            "effective_date": "2026-01-01T00:00:00",
        },
        headers=H1,
    )
    assert r.status_code == 400


def test_policy_scoping_and_filters():
    p = _make_policy()
    _make_policy(category="liquidity")

    assert len(client.get("/policies", headers=H1).json()) == 2
    # category filter
    assert len(client.get("/policies", params={"category": "fx_risk"}, headers=H1).json()) == 1
    assert len(client.get("/policies", params={"status": "active"}, headers=H1).json()) == 2
    # foreign caller + Book-gated: nothing visible
    assert client.get("/policies", headers=H2).json() == []
    other_book = dict(H1)
    other_book["X-Book-ID"] = BOOK_B
    assert client.get("/policies", headers=other_book).json() == []


def test_limit_and_check_scoping():
    p = _make_policy()
    lim = _make_limit(p["id"])

    # foreign caller cannot set limits on someone else's policy
    assert (
        client.post(
            f"/policies/{p['id']}/limits",
            params={"limit_type": "exposure", "limit_value": 5.0},
            headers=H2,
        ).status_code
        == 404
    )
    # limits listing scoped
    assert len(client.get(f"/policies/{p['id']}/limits", headers=H1).json()) == 1
    assert client.get(f"/policies/{p['id']}/limits", headers=H2).json() == []

    # compliance check: compliant + breach
    ok = client.post(f"/limits/{lim['id']}/check", params={"checked_value": 750000.0}, headers=H1).json()
    assert ok["compliant"] is True
    assert ok["utilization_pct"] == 75.0

    breach = client.post(f"/limits/{lim['id']}/check", params={"checked_value": 1100000.0}, headers=H1).json()
    assert breach["compliant"] is False

    # utilization persisted on the limit node
    lims = client.get(f"/policies/{p['id']}/limits", headers=H1).json()
    assert lims[0]["current_utilization"] == 1100000.0

    # foreign caller cannot run checks against this limit
    assert client.post(f"/limits/{lim['id']}/check", params={"checked_value": 1.0}, headers=H2).status_code == 404

    # compliance history scoped + tail limit
    checks = client.get("/compliance", headers=H1).json()
    assert len(checks) == 2
    assert all(c["compliant"] in (True, False) for c in checks)
    assert client.get("/compliance", params={"limit": 1}, headers=H1).json()[0]["checked_value"] == 1100000.0
    assert client.get("/compliance", headers=H2).json() == []
    other_book = dict(H1)
    other_book["X-Book-ID"] = BOOK_B
    assert client.get("/compliance", headers=other_book).json() == []
