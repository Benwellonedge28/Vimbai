"""Book-scoping and persistence tests for fund-accounting-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from fund_accounting_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("fund_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "fund-user-1", "fund-user-2"
BOOK_A, BOOK_B = "fund-book-a", "fund-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_fund(company="co-fund", name="Building Fund", ftype="restricted", balance=1000.0):
    return {
        "company_id": company,
        "fund_name": name,
        "fund_type": ftype,
        "balance": balance,
        "restrictions": "capital only",
        "manager": "Treasurer",
    }


def test_create_fund_net_assets_derived():
    resp = client.post("/funds", json=_mk_fund(balance=1000.0), headers=H1)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["net_assets"] == 1000.0
    assert body["book_id"] == BOOK_A


def test_funds_persist_and_totals():
    client.post("/funds", json=_mk_fund(name="A", balance=100.0), headers=H1)
    client.post("/funds", json=_mk_fund(name="B", balance=200.0), headers=H1)
    listed = client.get("/funds/co-fund", headers=H1).json()
    assert len(listed["funds"]) == 2
    assert listed["total_balance"] == 300.0
    assert listed["total_net_assets"] == 300.0


def test_fund_user_isolation():
    client.post("/funds", json=_mk_fund(), headers=H1)
    assert client.get("/funds/co-fund", headers=H2).json()["funds"] == []


def test_fund_book_a_b_isolation():
    client.post("/funds", json=_mk_fund(), headers=H1)
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/funds/co-fund", headers=other).json()["funds"] == []


def test_transaction_updates_fund_aggregates():
    fund = client.post("/funds", json=_mk_fund(balance=500.0), headers=H1).json()
    r1 = client.post(
        "/transactions",
        json={"fund_id": fund["id"], "description": "donation", "amount": 300.0, "is_income": True},
        headers=H1,
    )
    assert r1.status_code == 200, r1.text
    r2 = client.post(
        "/transactions",
        json={"fund_id": fund["id"], "description": "paint", "amount": 120.0, "is_income": False},
        headers=H1,
    )
    assert r2.status_code == 200

    funds = client.get("/funds/co-fund", headers=H1).json()["funds"]
    assert funds[0]["income"] == 300.0
    assert funds[0]["expenses"] == 120.0
    assert funds[0]["net_assets"] == 680.0  # 500 + 300 - 120


def test_transactions_persist_across_requests():
    fund = client.post("/funds", json=_mk_fund(), headers=H1).json()
    client.post(
        "/transactions",
        json={"fund_id": fund["id"], "description": "fee", "amount": 50.0, "is_income": False},
        headers=H1,
    )
    txs = client.get(f"/transactions/{fund['id']}", headers=H1).json()
    assert txs["total"] == 1
    assert txs["transactions"][0]["description"] == "fee"


def test_cross_user_transaction_404():
    fund = client.post("/funds", json=_mk_fund(), headers=H1).json()
    resp = client.post(
        "/transactions",
        json={"fund_id": fund["id"], "description": "sneaky", "amount": 999.0, "is_income": True},
        headers=H2,
    )
    assert resp.status_code == 404
    # and nothing was recorded
    assert client.get(f"/transactions/{fund['id']}", headers=H1).json()["total"] == 0


def test_cross_book_transaction_404():
    fund = client.post("/funds", json=_mk_fund(), headers=H1).json()
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    resp = client.post(
        "/transactions",
        json={"fund_id": fund["id"], "description": "cross-book", "amount": 10.0, "is_income": True},
        headers=other,
    )
    assert resp.status_code == 404


def test_cross_book_cannot_read_transactions():
    fund = client.post("/funds", json=_mk_fund(), headers=H1).json()
    client.post(
        "/transactions",
        json={"fund_id": fund["id"], "description": "income", "amount": 75.0, "is_income": True},
        headers=H1,
    )
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get(f"/transactions/{fund['id']}", headers=other).json()["total"] == 0
    # personal view still sees it
    assert client.get(f"/transactions/{fund['id']}", headers=H1_PERSONAL).json()["total"] == 1


def test_x_user_id_required():
    assert client.post("/funds", json=_mk_fund()).status_code in (401, 403, 422)
