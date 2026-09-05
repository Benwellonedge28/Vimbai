"""Book-scoping and persistence tests for cash-flow-statement-service (fake Neo4j harness)."""

import importlib.util
import os
import sys

import main
import pytest
from cash_flow_statement_service.database import Neo4jConnector
from fastapi.testclient import TestClient
from main import app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("cash_flow_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "cfs-user-1", "cfs-user-2"
BOOK_A, BOOK_B = "cfs-book-a", "cfs-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_stmt(company="co-cfs", beginning_cash=1000.0):
    return {
        "company_id": company,
        "method": "direct",
        "beginning_cash": beginning_cash,
        "operating_activities": [
            {"description": "Customer receipts", "amount": 5000.0, "is_inflow": True},
            {"description": "Salaries", "amount": 2000.0, "is_inflow": False},
        ],
        "investing_activities": [
            {"description": "Equipment purchase", "amount": 1500.0, "is_inflow": False},
        ],
        "financing_activities": [
            {"description": "Loan received", "amount": 3000.0, "is_inflow": True},
        ],
    }


def test_generate_computes_nets():
    resp = client.post("/generate", json=_mk_stmt(), headers=H1)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["net_operating"] == 3000.0
    assert body["net_investing"] == -1500.0
    assert body["net_financing"] == 3000.0
    assert body["net_change"] == 4500.0
    assert body["beginning_cash"] == 1000.0
    assert body["ending_cash"] == 5500.0


def test_generate_persists_across_requests():
    client.post("/generate", json=_mk_stmt(), headers=H1)
    latest = client.get("/latest/co-cfs", headers=H1)
    assert latest.status_code == 200
    assert latest.json()["ending_cash"] == 5500.0
    history = client.get("/history/co-cfs", headers=H1)
    assert history.json()["total"] == 1


def test_latest_returns_most_recent():
    client.post("/generate", json=_mk_stmt(), headers=H1)
    client.post("/generate", json=_mk_stmt(beginning_cash=9999.0), headers=H1)
    latest = client.get("/latest/co-cfs", headers=H1)
    assert latest.json()["beginning_cash"] == 9999.0
    history = client.get("/history/co-cfs", headers=H1)
    assert history.json()["total"] == 2
    assert history.json()["statements"][-1]["beginning_cash"] == 9999.0


def test_latest_404_when_empty():
    assert client.get("/latest/co-none", headers=H1).status_code == 404


def test_user_isolation():
    client.post("/generate", json=_mk_stmt(), headers=H1)
    assert client.get("/latest/co-cfs", headers=H2).status_code == 404


def test_book_a_b_isolation():
    client.post("/generate", json=_mk_stmt(), headers=H1)
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/latest/co-cfs", headers=other).status_code == 404
    assert client.get("/history/co-cfs", headers=other).json()["total"] == 0


def test_personal_view_spans_books():
    """Unscoped requests see records from every Book (own records only)."""
    client.post("/generate", json=_mk_stmt(), headers=H1)
    client.post(
        "/generate",
        json=_mk_stmt(company="co-cfs2"),
        headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
    )
    personal_a = client.get("/history/co-cfs", headers=H1_PERSONAL).json()["total"]
    personal_b = client.get("/history/co-cfs2", headers=H1_PERSONAL).json()["total"]
    assert personal_a == 1
    assert personal_b == 1
    in_a = client.get("/history/co-cfs", headers=H1).json()["total"]
    assert in_a == 1


def test_book_filter_hides_other_books_records():
    """A Book request must not see records created in another Book."""
    client.post("/generate", json=_mk_stmt(), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/history/co-cfs", headers=H1).json()["total"] == 0


def test_empty_activity_lists_ok():
    resp = client.post("/generate", json={"company_id": "co-empty", "beginning_cash": 0.0}, headers=H1)
    assert resp.status_code == 200, resp.text
    assert resp.json()["net_change"] == 0.0
    assert resp.json()["ending_cash"] == 0.0


def test_x_user_id_required():
    resp = client.post("/generate", json=_mk_stmt())
    assert resp.status_code in (401, 403, 422)
