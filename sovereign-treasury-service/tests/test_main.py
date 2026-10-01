"""Book-scoping and persistence tests for sovereign-treasury-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from sovereign_treasury_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("st_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "st-user-1", "st-user-2"
BOOK_A, BOOK_B = "st-book-a", "st-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _account(country="ZWE", account_type="stabilization_fund", balance=100000.0):
    return {"country": country, "account_type": account_type, "balance": balance, "currency": "USD"}


def _debt(country="ZWE", instrument="eurobond", outstanding=500000.0):
    return {
        "country": country,
        "instrument": instrument,
        "principal": 500000,
        "interest_rate": 7.5,
        "maturity_date": "2030-06-30T00:00:00Z",
        "outstanding": outstanding,
        "currency": "USD",
    }


def test_create_and_list_accounts():
    resp = client.post("/accounts", json=_account(), headers=H1)
    assert resp.status_code == 200, resp.text
    assert resp.json()["type"] == "stabilization_fund"
    client.post("/accounts", json=_account(account_type="foreign_reserves", balance=250000.0), headers=H1)

    data = client.get("/accounts/ZWE", headers=H1).json()
    assert data["country"] == "ZWE"
    assert len(data["accounts"]) == 2
    assert data["total_balance"] == 350000.0

    # user isolation: other user sees nothing
    assert client.get("/accounts/ZWE", headers=H2).json()["accounts"] == []


def test_accounts_book_isolation():
    client.post("/accounts", json=_account(country="ZWE"), headers=H1)
    client.post("/accounts", json=_account(country="ZWE"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert len(client.get("/accounts/ZWE", headers=H1).json()["accounts"]) == 1
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert len(client.get("/accounts/ZWE", headers=other).json()["accounts"]) == 1
    # personal spans books
    assert len(client.get("/accounts/ZWE", headers=H1_PERSONAL).json()["accounts"]) == 2


def test_register_and_list_debt():
    resp = client.post("/debt", json=_debt(), headers=H1)
    assert resp.status_code == 200, resp.text
    assert resp.json()["instrument"] == "eurobond"
    client.post("/debt", json=_debt(instrument="treasury_bill", outstanding=150000.0), headers=H1)

    data = client.get("/debt/ZWE", headers=H1).json()
    assert len(data["debts"]) == 2
    assert data["total_debt"] == 650000.0
    assert data["instruments"] == 2
    assert data["debts"][0]["outstanding"] == 500000.0

    # other user and other Book see nothing
    assert client.get("/debt/ZWE", headers=H2).json()["debts"] == []
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/debt/ZWE", headers=other_book).json()["debts"] == []


def test_fiscal_position_upsert_and_scope():
    payload = {
        "country": "ZWE",
        "fiscal_year": "2026",
        "total_revenue": 4000000.0,
        "total_expenditure": 4500000.0,
        "foreign_reserves": 800000.0,
    }
    resp = client.post("/fiscal-position", json=payload, headers=H1)
    assert resp.status_code == 200, resp.text
    pos = resp.json()
    assert pos["book_id"] == BOOK_A
    assert pos["fiscal_deficit"] == 500000.0

    # upsert overwrites same user+country position
    payload["total_revenue"] = 4600000.0
    pos2 = client.post("/fiscal-position", json=payload, headers=H1).json()
    assert pos2["fiscal_deficit"] == -100000.0
    all_pos = client.get("/fiscal-position/ZWE", headers=H1).json()
    assert all_pos["total_revenue"] == 4600000.0

    # other user 404s, other Book 404s
    assert client.get("/fiscal-position/ZWE", headers=H2).status_code == 404
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/fiscal-position/ZWE", headers=other_book).status_code == 404
    # personal view sees it
    assert client.get("/fiscal-position/ZWE", headers=H1_PERSONAL).status_code == 200


def test_fiscal_position_missing_404():
    assert client.get("/fiscal-position/NAM", headers=H1).status_code == 404


def test_x_user_id_required():
    assert client.post("/accounts", json=_account()).status_code in (401, 403, 422)
    assert client.get("/accounts/ZWE").status_code in (401, 403, 422)
