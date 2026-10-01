"""Book-scoping and persistence tests for treasury-management-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from treasury_management_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("tm_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "tm-user-1", "tm-user-2"
BOOK_A, BOOK_B = "tm-book-a", "tm-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _flow(company="co-tm", ftype="inflow", amount=50000.0, **kw):
    payload = {"company_id": company, "flow_type": ftype, "amount": amount}
    payload.update(kw)
    return payload


def test_record_cashflow_returns_id():
    resp = client.post("/cashflows", json=_flow(), headers=H1)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["status"] == "recorded"
    assert body["id"]


def test_cashflows_persist_with_limit_and_total():
    for i in range(5):
        client.post("/cashflows", json=_flow(amount=1000.0 + i), headers=H1)
    body = client.get("/cashflows/co-tm", headers=H1).json()
    assert body["total"] == 5
    assert len(body["cashflows"]) == 5
    limited = client.get("/cashflows/co-tm", params={"limit": 2}, headers=H1).json()
    assert len(limited["cashflows"]) == 2
    assert limited["total"] == 5
    # most recent last
    assert limited["cashflows"][-1]["amount"] == 1004.0


def test_user_isolation():
    client.post("/cashflows", json=_flow(), headers=H1)
    other = client.get("/cashflows/co-tm", headers=H2).json()
    assert other["total"] == 0
    assert other["cashflows"] == []


def test_book_a_b_isolation():
    client.post("/cashflows", json=_flow(company="co-a"), headers=H1)
    client.post("/cashflows", json=_flow(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/cashflows/co-a", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()["total"] == 0
    assert client.get("/cashflows/co-a", headers=H1).json()["total"] == 1


def test_personal_view_spans_books():
    client.post("/cashflows", json=_flow(company="co-a"), headers=H1)
    client.post("/cashflows", json=_flow(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/cashflows/co-a", headers=H1_PERSONAL).json()["total"] == 1
    assert client.get("/cashflows/co-b", headers=H1_PERSONAL).json()["total"] == 1


def test_position_derived_from_flows():
    client.post("/cashflows", json=_flow(amount=50000.0), headers=H1)
    client.post("/cashflows", json=_flow(ftype="outflow", amount=20000.0), headers=H1)
    pos = client.get("/position/co-tm", headers=H1).json()
    assert pos["total_cash"] == 30000.0
    assert pos["available_cash"] == 30000.0
    # other user: nothing visible -> zero position
    other = client.get("/position/co-tm", headers=H2).json()
    assert other["total_cash"] == 0


def test_position_update_persists_and_scopes():
    client.post("/cashflows", json=_flow(amount=50000.0), headers=H1)
    put = client.put("/position/co-tm", json={"total_cash": 100000.0, "available_cash": 80000.0}, headers=H1)
    assert put.status_code == 200, put.text
    body = put.json()
    assert body["liquidity_level"] == "excess"
    assert body["book_id"] == BOOK_A

    # persisted, no longer derived from flows
    got = client.get("/position/co-tm", headers=H1).json()
    assert got["total_cash"] == 100000.0
    assert got["available_cash"] == 80000.0

    # other Book cannot see the stored position nor the Book-A flows -> zero derived view
    other = client.get("/position/co-tm", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()
    assert other["total_cash"] == 0.0
    assert other["available_cash"] == 0.0

    # cross-user cannot overwrite
    blocked = client.put("/position/co-tm", json={"total_cash": 1.0, "available_cash": 1.0}, headers=H2)
    assert blocked.status_code == 200  # creates their OWN separate position, not touching U1's
    u1_pos = client.get("/position/co-tm", headers=H1).json()
    assert u1_pos["total_cash"] == 100000.0


def test_liquidity_tight_with_recent_burn():
    client.post("/cashflows", json=_flow(ftype="outflow", amount=60000.0), headers=H1)
    put = client.put("/position/co-tm", json={"total_cash": 100000.0, "available_cash": 100000.0}, headers=H1)
    # 100k / 60k monthly burn = ~1.7 months runway -> tight
    assert put.json()["liquidity_level"] == "tight"


def test_forecast_from_history():
    client.post("/cashflows", json=_flow(amount=30000.0, description="first"), headers=H1)
    fc = client.post("/forecast/co-tm", params={"days": 30}, headers=H1)
    assert fc.status_code == 200, fc.text
    data = fc.json()
    assert data["projected_inflows"] >= 0
    assert data["projected_outflows"] >= 0
    assert "assumptions" in data
    # other user gets the zero-based forecast
    other = client.post("/forecast/co-tm", params={"days": 30}, headers=H2).json()
    assert other["projected_inflows"] == 0
    assert other["ending_position"] == 0


def test_investment_options_static():
    resp = client.get("/investment-options")
    assert resp.status_code == 200
    assert len(resp.json()["options"]) >= 3


def test_x_user_id_required():
    assert client.post("/cashflows", json=_flow()).status_code in (401, 403, 422)
