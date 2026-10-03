"""Book-scoping and persistence tests for general-reserve-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from general_reserve_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("gr_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "gr-user-1", "gr-user-2"
BOOK_A, BOOK_B = "gr-book-a", "gr-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_reserve(client, headers, **kw):
    params = {"company_id": "co-1", "reserve_name": "Contingency Reserve", "initial_balance": 10000.0}
    params.update(kw)
    return client.post("/reserves/create", params=params, headers=headers)


def test_health():
    assert client.get("/health").json()["status"] == "healthy"


def test_create_and_isolation():
    r = _mk_reserve(client, H1)
    assert r.status_code == 200, r.text
    reserve = r.json()
    assert reserve["current_balance"] == 10000.0

    _mk_reserve(client, H2, reserve_name="Other User Reserve")
    _mk_reserve(client, {"X-User-Id": U1, "X-Book-ID": BOOK_B}, reserve_name="Book B Reserve")

    assert len(client.get("/reserves", headers=H1).json()["reserves"]) == 1
    assert len(client.get("/reserves", headers=H2).json()["reserves"]) == 1
    assert client.get(f"/reserves/{reserve['id']}", headers=H2).json() == {"error": "Reserve not found"}
    got = client.get(f"/reserves/{reserve['id']}", headers=H1).json()
    assert got["reserve_name"] == "Contingency Reserve"


def test_allocate_and_utilize_scoped():
    reserve = _mk_reserve(client, H1).json()

    # cross-user allocate: parent invisible -> miss
    r = client.post(
        f"/reserves/{reserve['id']}/allocate",
        params={"amount": 5000.0, "source": "retained_earnings", "description": "Top-up"},
        headers=H2,
    )
    assert r.json() == {"error": "Reserve not found"}

    # owner allocate
    r = client.post(
        f"/reserves/{reserve['id']}/allocate",
        params={"amount": 5000.0, "source": "profit", "description": "Top-up"},
        headers=H1,
    )
    assert r.status_code == 200, r.text
    assert r.json()["reserve"]["current_balance"] == 15000.0

    # insufficient balance contract kept
    r = client.post(
        f"/reserves/{reserve['id']}/utilize",
        params={"amount": 99999.0, "purpose": "asset_purchase", "description": "Too much"},
        headers=H1,
    )
    assert r.json() == {"error": "Insufficient reserve balance"}

    # owner utilize
    r = client.post(
        f"/reserves/{reserve['id']}/utilize",
        params={"amount": 6000.0, "purpose": "debt_repayment", "description": "Loan payoff"},
        headers=H1,
    )
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["reserve"]["current_balance"] == 9000.0
    assert body["utilization"]["purpose"] == "debt_repayment"

    # balance persisted
    assert client.get(f"/reserves/{reserve['id']}", headers=H1).json()["current_balance"] == 9000.0

    # history scoped to caller
    h = client.get(f"/reserves/{reserve['id']}/history", headers=H1).json()
    assert len(h["allocations"]) == 1
    assert len(h["utilizations"]) == 1
    h2 = client.get(f"/reserves/{reserve['id']}/history", headers=H2).json()
    assert h2["allocations"] == [] and h2["utilizations"] == []
