"""Book-scoping and persistence tests for cash-management-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from cash_management_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("cm_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "cm-user-1", "cm-user-2"
BOOK_A, BOOK_B = "cm-book-a", "cm-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_account(client, headers, **kw):
    params = {"account_name": "Main", "bank": "Stanbic", "account_number": "123456", "balance": 1000.0}
    params.update(kw)
    return client.post("/accounts", params=params, headers=headers)


def test_health():
    for path in ("/", "/health"):
        assert client.get(path).json()["status"] == "healthy"


def test_account_crud_and_isolation():
    resp = _mk_account(client, H1, balance=1000.0, min_balance=100.0, type="operating")
    assert resp.status_code == 200, resp.text
    acct = resp.json()
    assert acct["balance"] == 1000.0
    assert acct["type"] == "operating"

    # invalid type: 400
    assert _mk_account(client, H1, type="gambling").status_code == 400

    # caller sees it; other user / other Book do not
    assert len(client.get("/accounts", headers=H1).json()) == 1
    assert client.get("/accounts", headers=H2).json() == []
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/accounts", headers=other_book).json() == []
    # personal (no Book) scope sees the caller's own records across Books
    assert len(client.get("/accounts", headers=H1_PERSONAL).json()) == 1

    # type filter
    _mk_account(client, H1, account_name="Reserve", type="reserve", balance=500.0)
    only_reserve = client.get("/accounts", params={"type": "reserve"}, headers=H1).json()
    assert len(only_reserve) == 1 and only_reserve[0]["type"] == "reserve"


def test_transfer_semantics_and_isolation():
    src = _mk_account(client, H1, balance=1000.0, min_balance=100.0).json()
    dst = _mk_account(client, H1, account_name="Dest", balance=50.0).json()

    # transfer moves balances and persists
    resp = client.post(
        "/transfers",
        params={"from_account_id": src["id"], "to_account_id": dst["id"], "amount": 300.0, "reference": "t1"},
        headers=H1,
    )
    assert resp.status_code == 200, resp.text
    assert resp.json()["status"] == "completed"

    accounts = {a["id"]: a for a in client.get("/accounts", headers=H1).json()}
    assert accounts[src["id"]]["balance"] == 700.0
    assert accounts[dst["id"]]["balance"] == 350.0

    # min-balance breach: 400
    assert (
        client.post(
            "/transfers",
            params={"from_account_id": src["id"], "to_account_id": dst["id"], "amount": 650.0},
            headers=H1,
        ).status_code
        == 400
    )

    # unknown account: 404
    assert (
        client.post(
            "/transfers",
            params={"from_account_id": src["id"], "to_account_id": "nope", "amount": 1.0},
            headers=H1,
        ).status_code
        == 404
    )

    # another user cannot transfer from my account (404: not visible)
    assert (
        client.post(
            "/transfers",
            params={"from_account_id": src["id"], "to_account_id": dst["id"], "amount": 100.0},
            headers=H2,
        ).status_code
        == 404
    )

    # other user / other Book see no transfers
    assert client.get("/transfers", headers=H2).json() == []
    assert client.get("/transfers", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json() == []
    assert len(client.get("/transfers", headers=H1).json()) == 1

    # balances unchanged by the failed cross-scope attempt
    accounts = {a["id"]: a for a in client.get("/accounts", headers=H1).json()}
    assert accounts[src["id"]]["balance"] == 700.0


def test_liquidity_scoped_to_caller():
    _mk_account(client, H1, balance=1000.0, type="operating")
    _mk_account(client, H1, account_name="Res", balance=400.0, type="reserve")
    _mk_account(client, H1, account_name="Inv", balance=600.0, type="investment")

    # other user's cash must not leak into the ratio
    _mk_account(client, H2, account_name="Foreign", balance=999999.0, type="operating")

    resp = client.post("/liquidity", params={"short_term_obligations": 2000.0}, headers=H1)
    assert resp.status_code == 200, resp.text
    pos = resp.json()
    assert pos["operating_cash"] == 1000.0
    assert pos["reserve_cash"] == 400.0
    assert pos["invested_cash"] == 600.0
    assert pos["total_cash"] == 2000.0
    assert pos["liquidity_ratio"] == 1.0

    # history is caller-scoped
    assert len(client.get("/liquidity", headers=H1).json()) == 1
    assert client.get("/liquidity", headers=H2).json() == []

    # zero obligations: ratio 0 (original semantics)
    pos = client.post("/liquidity", headers=H1).json()
    assert pos["liquidity_ratio"] == 0.0
    assert len(client.get("/liquidity", headers=H1).json()) == 2
