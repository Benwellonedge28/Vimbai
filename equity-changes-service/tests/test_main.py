"""Book-scoping and persistence tests for equity-changes-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from equity_changes_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("equity_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "eq-user-1", "eq-user-2"
BOOK_A, BOOK_B = "eq-book-a", "eq-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_tx(company="co-eq", tx_type="issuance", shares=100, price=10.0, amount=None):
    d = {
        "company_id": company,
        "transaction_type": tx_type,
        "shareholder": "Holder One",
        "shares": shares,
        "price_per_share": price,
        "description": "test tx",
    }
    if amount is not None:
        d["amount"] = amount
    return d


def test_create_transaction_amount_derivation():
    resp = client.post("/transactions", json=_mk_tx(), headers=H1)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["amount"] == 1000.0  # shares * price when amount unset
    assert body["book_id"] == BOOK_A


def test_create_transaction_explicit_amount_kept():
    resp = client.post("/transactions", json=_mk_tx(amount=555.0), headers=H1)
    assert resp.json()["amount"] == 555.0


def test_transactions_persist_across_requests():
    client.post("/transactions", json=_mk_tx(), headers=H1)
    listed = client.get("/transactions/co-eq", headers=H1).json()
    assert listed["total"] == 1
    assert listed["transactions"][0]["shareholder"] == "Holder One"


def test_tx_type_filter():
    client.post("/transactions", json=_mk_tx(tx_type="issuance"), headers=H1)
    client.post("/transactions", json=_mk_tx(tx_type="dividend", amount=200.0), headers=H1)
    assert client.get("/transactions/co-eq", headers=H1).json()["total"] == 2
    divs = client.get("/transactions/co-eq", params={"tx_type": "dividend"}, headers=H1).json()
    assert divs["total"] == 1
    assert divs["transactions"][0]["transaction_type"] == "dividend"


def test_user_isolation():
    client.post("/transactions", json=_mk_tx(), headers=H1)
    assert client.get("/transactions/co-eq", headers=H2).json()["total"] == 0


def test_book_a_b_isolation():
    client.post("/transactions", json=_mk_tx(), headers=H1)
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/transactions/co-eq", headers=other).json()["total"] == 0


def test_personal_view_spans_books():
    client.post("/transactions", json=_mk_tx(company="co-a"), headers=H1)
    client.post(
        "/transactions",
        json=_mk_tx(company="co-b"),
        headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
    )
    assert client.get("/transactions/co-a", headers=H1_PERSONAL).json()["total"] == 1
    assert client.get("/transactions/co-b", headers=H1_PERSONAL).json()["total"] == 1


def test_book_cannot_see_other_books_records():
    client.post("/transactions", json=_mk_tx(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/transactions/co-b", headers=H1).json()["total"] == 0


def test_statement_rollup_and_persistence():
    payload = {
        "company_id": "co-eq",
        "period": "2026-Q1",
        "beginning_equity": 50000.0,
        "transactions": [
            _mk_tx(tx_type="issuance", shares=1000, price=5.0),
            _mk_tx(tx_type="buyback", shares=200, price=5.0),
            _mk_tx(tx_type="dividend", amount=3000.0),
            _mk_tx(tx_type="retained_earnings", amount=8000.0),
            _mk_tx(tx_type="split", amount=500.0),
        ],
    }
    resp = client.post("/statement", json=payload, headers=H1)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["share_issuances"] == 5000.0
    assert body["share_buybacks"] == 1000.0
    assert body["dividends_paid"] == 3000.0
    assert body["retained_earnings_change"] == 8000.0
    assert body["other_changes"] == 500.0
    assert body["ending_equity"] == 59500.0

    listed = client.get("/statements/co-eq", headers=H1).json()
    assert listed["total"] == 1
    assert listed["statements"][0]["ending_equity"] == 59500.0
    assert len(listed["statements"][0]["transactions"]) == 5


def test_statements_user_and_book_isolation():
    client.post(
        "/statement",
        json={"company_id": "co-eq", "period": "P", "beginning_equity": 100.0},
        headers=H1,
    )
    assert client.get("/statements/co-eq", headers=H2).json()["total"] == 0
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/statements/co-eq", headers=other).json()["total"] == 0
    assert client.get("/statements/co-eq", headers=H1_PERSONAL).json()["total"] == 1


def test_x_user_id_required():
    assert client.post("/transactions", json=_mk_tx()).status_code in (401, 403, 422)
