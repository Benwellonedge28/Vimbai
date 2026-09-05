"""Book-scoping and persistence tests for debt-management-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from debt_management_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("debt_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "debt-user-1", "debt-user-2"
BOOK_A, BOOK_B = "debt-book-a", "debt-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_loan(company="co-debt", name="Bank Loan", principal=100000.0, rate=0.10, months=36):
    return {
        "company_id": company,
        "loan_name": name,
        "lender": "Stanbic",
        "principal": principal,
        "interest_rate": rate,
        "term_months": months,
        "disbursement_date": "2026-01-01",
    }


def test_create_loan_remaining_balance_derived():
    resp = client.post("/loans", json=_mk_loan(), headers=H1)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["remaining_balance"] == 100000.0
    assert body["book_id"] == BOOK_A


def test_loans_persist_and_scope_filters():
    client.post("/loans", json=_mk_loan(name="L1"), headers=H1)
    client.post("/loans", json=_mk_loan(name="L2"), headers=H1)
    listed = client.get("/loans", params={"company_id": "co-debt"}, headers=H1).json()
    assert len(listed) == 2
    assert {l["loan_name"] for l in listed} == {"L1", "L2"}


def test_user_isolation():
    client.post("/loans", json=_mk_loan(), headers=H1)
    assert client.get("/loans", params={"company_id": "co-debt"}, headers=H2).json() == []


def test_book_a_b_isolation():
    client.post("/loans", json=_mk_loan(), headers=H1)
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/loans", params={"company_id": "co-debt"}, headers=other).json() == []


def test_personal_view_spans_books():
    client.post("/loans", json=_mk_loan(company="co-a"), headers=H1)
    client.post("/loans", json=_mk_loan(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert len(client.get("/loans", params={"company_id": "co-a"}, headers=H1_PERSONAL).json()) == 1
    assert len(client.get("/loans", params={"company_id": "co-b"}, headers=H1_PERSONAL).json()) == 1


def test_schedule_computed_and_scoped():
    loan = client.post("/loans", json=_mk_loan(), headers=H1).json()
    resp = client.post(f"/loans/{loan['id']}/schedule", params={"company_id": "co-debt"}, headers=H1)
    assert resp.status_code == 200, resp.text
    sched = resp.json()
    assert len(sched) == 36
    assert sched[0]["interest_component"] > sched[-1]["interest_component"]

    # other user / other Book get 404
    assert client.post(f"/loans/{loan['id']}/schedule", params={"company_id": "co-debt"}, headers=H2).status_code == 404
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert (
        client.post(f"/loans/{loan['id']}/schedule", params={"company_id": "co-debt"}, headers=other).status_code == 404
    )


def test_summary_computed_from_visible_loans():
    client.post("/loans", json=_mk_loan(principal=50000.0, rate=0.08, months=24), headers=H1)
    resp = client.get("/summary", params={"company_id": "co-debt", "equity": 200000}, headers=H1)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["total_debt"] == 50000.0
    assert body["debt_to_equity"] == 0.25
    assert body["weighted_avg_rate"] == 0.08
    assert len(body["loans"]) == 1

    # other user sees a zeroed summary for the same company
    other_body = client.get("/summary", params={"company_id": "co-debt"}, headers=H2).json()
    assert other_body["total_debt"] == 0
    assert other_body["loans"] == []


def test_x_user_id_required():
    assert client.post("/loans", json=_mk_loan()).status_code in (401, 403, 422)
