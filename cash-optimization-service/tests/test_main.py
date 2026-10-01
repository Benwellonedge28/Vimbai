"""Book-scoping and persistence tests for cash-optimization-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from cash_optimization_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("co_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "co-user-1", "co-user-2"
BOOK_A, BOOK_B = "co-book-a", "co-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def test_add_and_list_accounts():
    resp = client.post(
        "/accounts",
        json={"company_id": "co-x", "account_name": "Operating", "account_type": "operating", "balance": 200000},
        headers=H1,
    )
    assert resp.status_code == 200, resp.text
    assert resp.json()["account_name"] == "Operating"

    listed = client.get("/accounts/co-x", headers=H1).json()
    assert len(listed["accounts"]) == 1
    assert listed["accounts"][0]["balance"] == 200000.0
    assert listed["accounts"][0]["book_id"] == BOOK_A

    # other user sees nothing
    assert client.get("/accounts/co-x", headers=H2).json()["accounts"] == []


def test_optimize_and_suggestions_persist():
    client.post(
        "/accounts",
        json={
            "company_id": "co-y",
            "account_name": "Operating",
            "account_type": "operating",
            "balance": 200000,
            "min_required": 50000,
        },
        headers=H1,
    )
    client.post(
        "/accounts",
        json={
            "company_id": "co-y",
            "account_name": "Investment",
            "account_type": "investment",
            "balance": 50000,
            "interest_rate": 0.05,
        },
        headers=H1,
    )
    resp = client.post("/optimize/co-y", headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["total_count"] >= 1
    assert data["potential_annual_benefit"] > 0

    stored = client.get("/suggestions/co-y", headers=H1).json()
    assert len(stored["suggestions"]) == data["total_count"]

    # other user / other Book see none of it
    assert client.get("/suggestions/co-y", headers=H2).json()["suggestions"] == []
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/suggestions/co-y", headers=other_book).json()["suggestions"] == []


def test_optimize_scoped_to_caller_accounts():
    # user 2 seeds a fat operating account for the same company name
    client.post(
        "/accounts",
        json={
            "company_id": "co-z",
            "account_name": "U2 Operating",
            "account_type": "operating",
            "balance": 999999,
            "min_required": 1000,
        },
        headers=H2,
    )
    client.post(
        "/accounts",
        json={
            "company_id": "co-z",
            "account_name": "U2 Investment",
            "account_type": "investment",
            "balance": 1,
            "interest_rate": 0.2,
        },
        headers=H2,
    )
    # user 1 optimizes the same company - must NOT see user 2's accounts
    resp = client.post("/optimize/co-z", headers=H1).json()
    assert resp["total_count"] == 0


def test_rerun_replaces_suggestions():
    client.post(
        "/accounts",
        json={
            "company_id": "co-r",
            "account_name": "Operating",
            "account_type": "operating",
            "balance": 200000,
            "min_required": 50000,
        },
        headers=H1,
    )
    client.post(
        "/accounts",
        json={
            "company_id": "co-r",
            "account_name": "Investment",
            "account_type": "investment",
            "balance": 50000,
            "interest_rate": 0.05,
        },
        headers=H1,
    )
    first = client.post("/optimize/co-r", headers=H1).json()
    assert first["total_count"] >= 1
    second = client.post("/optimize/co-r", headers=H1).json()
    stored = client.get("/suggestions/co-r", headers=H1).json()
    assert len(stored["suggestions"]) == second["total_count"] == first["total_count"]


def test_book_a_b_isolation():
    client.post(
        "/accounts",
        json={"company_id": "co-a", "account_name": "A Operating", "account_type": "operating", "balance": 100000},
        headers=H1,
    )
    client.post(
        "/accounts",
        json={"company_id": "co-a", "account_name": "B Operating", "account_type": "operating", "balance": 90000},
        headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
    )
    # Book A sees only its account
    names_a = [a["account_name"] for a in client.get("/accounts/co-a", headers=H1).json()["accounts"]]
    assert names_a == ["A Operating"]
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    names_b = [a["account_name"] for a in client.get("/accounts/co-a", headers=other_book).json()["accounts"]]
    assert names_b == ["B Operating"]
    # personal sees both
    names_p = [a["account_name"] for a in client.get("/accounts/co-a", headers=H1_PERSONAL).json()["accounts"]]
    assert set(names_p) == {"A Operating", "B Operating"}


def test_underfunded_topup_suggestion():
    client.post(
        "/accounts",
        json={
            "company_id": "co-t",
            "account_name": "Op",
            "account_type": "operating",
            "balance": 100000,
            "min_required": 10000,
        },
        headers=H1,
    )
    client.post(
        "/accounts",
        json={
            "company_id": "co-t",
            "account_name": "Tax",
            "account_type": "tax",
            "balance": 5000,
            "min_required": 8000,
        },
        headers=H1,
    )
    resp = client.post("/optimize/co-t", headers=H1).json()
    topups = [s for s in resp["suggestions"] if s["to_account"] == "Tax"]
    assert len(topups) == 1
    assert topups[0]["amount"] == 3000.0
    assert topups[0]["priority"] == "high"


def test_x_user_id_required():
    bad = {"company_id": "c", "account_name": "n", "account_type": "operating", "balance": 1}
    assert client.post("/accounts", json=bad).status_code in (401, 403, 422)
    assert client.get("/accounts/c").status_code in (401, 403, 422)
