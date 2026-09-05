"""Book-scoping and persistence tests for balance-sheet-service (fake Neo4j harness)."""

import importlib.util
import os
import sys

import main
import pytest
from balance_sheet_service.database import Neo4jConnector
from fastapi.testclient import TestClient
from main import app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("balance_sheet_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession

_fake_session = FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    """Each test starts from an empty graph (fresh per-request state)."""
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "bs-user-1", "bs-user-2"
BOOK_A, BOOK_B = "bs-book-a", "bs-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_sheet(company="co-bs", assets=None, liabilities=None, equity=None):
    assets = (
        assets
        if assets is not None
        else [
            {"name": "Cash", "amount": 6000.0, "category": "current", "is_liquid": True},
            {"name": "Plant", "amount": 4000.0, "category": "non_current", "is_liquid": False},
        ]
    )
    liabilities = (
        liabilities
        if liabilities is not None
        else [
            {"name": "Payables", "amount": 3000.0, "category": "current"},
        ]
    )
    equity = (
        equity
        if equity is not None
        else [
            {"name": "Share capital", "amount": 7000.0},
        ]
    )
    return {"company_id": company, "assets": assets, "liabilities": liabilities, "equity": equity}


def test_generate_computes_totals_and_balances():
    resp = client.post("/generate", json=_mk_sheet(), headers=H1)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["total_assets"] == 10000.0
    assert body["total_liabilities"] == 3000.0
    assert body["total_equity"] == 7000.0
    assert body["is_balanced"] is True


def test_generate_persists_across_requests():
    client.post("/generate", json=_mk_sheet(), headers=H1)
    latest = client.get("/latest/co-bs", headers=H1)
    assert latest.status_code == 200
    assert latest.json()["total_assets"] == 10000.0
    history = client.get("/history/co-bs", headers=H1)
    assert history.json()["total"] == 1


def test_latest_returns_most_recent():
    client.post("/generate", json=_mk_sheet(), headers=H1)
    client.post(
        "/generate",
        json=_mk_sheet(assets=[{"name": "Cash", "amount": 9000.0}], liabilities=[], equity=[]),
        headers=H1,
    )
    latest = client.get("/latest/co-bs", headers=H1)
    assert latest.json()["total_assets"] == 9000.0
    history = client.get("/history/co-bs", headers=H1)
    assert history.json()["total"] == 2
    assert history.json()["sheets"][-1]["total_assets"] == 9000.0


def test_ratios_computed_from_latest():
    client.post("/generate", json=_mk_sheet(), headers=H1)
    resp = client.get("/ratios/co-bs", headers=H1)
    assert resp.status_code == 200
    body = resp.json()
    assert body["current_ratio"] == 2.0
    assert body["quick_ratio"] == 2.0


def test_latest_404_when_empty():
    assert client.get("/latest/co-none", headers=H1).status_code == 404
    assert client.get("/ratios/co-none", headers=H1).status_code == 404


def test_user_isolation():
    client.post("/generate", json=_mk_sheet(), headers=H1)
    assert client.get("/latest/co-bs", headers=H2).status_code == 404


def test_book_a_b_isolation():
    client.post("/generate", json=_mk_sheet(), headers=H1)
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/latest/co-bs", headers=other).status_code == 404
    assert client.get("/history/co-bs", headers=other).json()["total"] == 0


def test_personal_view_spans_books():
    """Unscoped requests see records from every Book (own records only)."""
    client.post("/generate", json=_mk_sheet(), headers=H1)
    client.post(
        "/generate",
        json=_mk_sheet(company="co-bs2"),
        headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
    )
    personal_a = client.get("/history/co-bs", headers=H1_PERSONAL).json()["total"]
    personal_b = client.get("/history/co-bs2", headers=H1_PERSONAL).json()["total"]
    assert personal_a == 1
    assert personal_b == 1
    in_a = client.get("/history/co-bs", headers=H1).json()["total"]
    assert in_a == 1


def test_book_filter_hides_other_books_records():
    """A Book request must not see records created in another Book."""
    client.post("/generate", json=_mk_sheet(), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/history/co-bs", headers=H1).json()["total"] == 0


def test_unbalanced_sheet_flagged():
    resp = client.post(
        "/generate",
        json=_mk_sheet(
            assets=[{"name": "Cash", "amount": 500.0}],
            liabilities=[{"name": "Loan", "amount": 100.0}],
            equity=[{"name": "Capital", "amount": 100.0}],
        ),
        headers=H1,
    )
    assert resp.json()["is_balanced"] is False


def test_x_user_id_required():
    resp = client.post("/generate", json=_mk_sheet())
    assert resp.status_code in (401, 403, 422)
