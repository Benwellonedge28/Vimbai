"""Book-scoping and persistence tests for partnership-revaluation-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from partnership_revaluation_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("pr_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession

_fake_session = FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "pr-user-1", "pr-user-2"
BOOK_A, BOOK_B = "pr-book-a", "pr-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}

ASSET_REVALS = [
    {
        "asset_id": "A1",
        "asset_name": "Premises",
        "asset_code": "PREM",
        "account_code": "1600",
        "old_value": 80000.0,
        "new_value": 100000.0,
    },
    {
        "asset_id": "A2",
        "asset_name": "Vehicle",
        "asset_code": "VEH",
        "account_code": "1700",
        "old_value": 30000.0,
        "new_value": 24000.0,
    },
]


def _mk_revalue(client, headers, **kw):
    params = {
        "partnership_id": "ps-1",
        "revaluation_date": "2026-02-01",
        "goodwill_treatment": "raise_and_raise",
    }
    params.update(kw)
    return client.post(
        "/revalue",
        params=params,
        json=ASSET_REVALS,
        headers=headers,
    )


def test_health():
    assert client.get("/health").json()["status"] == "healthy"


def test_revalue_math_and_persistence():
    r = _mk_revalue(client, H1)
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["total_increase"] == 20000.0
    assert body["total_decrease"] == 6000.0
    assert body["net_gain"] == 14000.0
    assert len(body["entries"]) == 2
    assert body["entries"][0]["increase"] == 20000.0
    assert body["entries"][1]["decrease"] == 6000.0

    # persisted, caller-scoped
    listed = client.get("/revaluations", headers=H1).json()
    assert listed["count"] == 1
    assert listed["revaluations"][0]["net_gain"] == 14000.0
    assert client.get("/revaluations", headers=H2).json()["count"] == 0

    # partnership filter
    assert client.get("/revaluations", params={"partnership_id": "ps-1"}, headers=H1).json()["count"] == 1
    assert client.get("/revaluations", params={"partnership_id": "ps-x"}, headers=H1).json()["count"] == 0


def test_isolation_and_lookup_miss():
    d1 = _mk_revalue(client, H1).json()
    _mk_revalue(client, H2, partnership_id="ps-2")
    _mk_revalue(client, {"X-User-Id": U1, "X-Book-ID": BOOK_B}, partnership_id="ps-3")

    # cross-scope lookup behaves as not-found (original miss contract kept)
    assert client.get(f"/revaluations/{d1['id']}", headers=H2).json() == {"error": "Revaluation not found"}
    assert client.get(f"/revaluations/{d1['id']}", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json() == {
        "error": "Revaluation not found"
    }
    got = client.get(f"/revaluations/{d1['id']}", headers=H1).json()
    assert got["id"] == d1["id"]
    assert got["total_increase"] == 20000.0
    assert client.get("/revaluations/none", headers=H1).json() == {"error": "Revaluation not found"}
