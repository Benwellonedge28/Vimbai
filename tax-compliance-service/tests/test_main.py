"""Book-scoping and persistence tests for tax-compliance-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from tax_compliance_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("tc_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "tc-user-1", "tc-user-2"
BOOK_A, BOOK_B = "tc-book-a", "tc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _payload(company="co-tc", **kw):
    p = {
        "company_id": company,
        "obligation_type": "vat_return",
        "description": "Q1 VAT Return",
        "due_date": "2026-04-30T00:00:00Z",
        "amount": 15000,
        "filing_frequency": "quarterly",
    }
    p.update(kw)
    return p


def test_create_list_persist():
    resp = client.post("/obligations", json=_payload(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["status"] == "pending"
    assert data["amount"] == 15000

    listed = client.get("/obligations", params={"company_id": "co-tc"}, headers=H1).json()
    assert len(listed) == 1
    assert listed[0]["id"] == data["id"]
    # persists across requests
    assert len(client.get("/obligations", params={"company_id": "co-tc"}, headers=H1).json()) == 1
    # other user sees nothing
    assert client.get("/obligations", params={"company_id": "co-tc"}, headers=H2).json() == []


def test_status_filter():
    client.post("/obligations", json=_payload(description="A"), headers=H1)
    oid = client.post("/obligations", json=_payload(description="B"), headers=H1).json()["id"]
    client.post(f"/obligations/{oid}/file", params={"company_id": "co-tc", "filed_amount": 100}, headers=H1)
    pending = client.get("/obligations", params={"company_id": "co-tc", "status": "pending"}, headers=H1).json()
    filed = client.get("/obligations", params={"company_id": "co-tc", "status": "filed"}, headers=H1).json()
    assert len(pending) == 1
    assert len(filed) == 1
    assert filed[0]["amount"] == 100


def test_file_persists_and_scopes():
    oid = client.post("/obligations", json=_payload(), headers=H1).json()["id"]
    filed = client.post(f"/obligations/{oid}/file", params={"company_id": "co-tc", "filed_amount": 15000}, headers=H1)
    assert filed.status_code == 200, filed.text
    assert filed.json() == {"filed": True, "obligation_id": oid, "amount": 15000}
    stored = client.get("/obligations", params={"company_id": "co-tc"}, headers=H1).json()
    assert stored[0]["status"] == "filed"

    # cross-user file 404
    oid2 = client.post("/obligations", json=_payload(), headers=H1).json()["id"]
    assert (
        client.post(
            f"/obligations/{oid2}/file", params={"company_id": "co-tc", "filed_amount": 1}, headers=H2
        ).status_code
        == 404
    )
    # cross-Book file 404
    assert (
        client.post(
            f"/obligations/{oid2}/file",
            params={"company_id": "co-tc", "filed_amount": 1},
            headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
        ).status_code
        == 404
    )
    # wrong company 404
    assert (
        client.post(
            f"/obligations/{oid2}/file", params={"company_id": "other-co", "filed_amount": 1}, headers=H1
        ).status_code
        == 404
    )


def test_summary_semantics():
    client.post("/obligations", json=_payload(), headers=H1)
    oid = client.post("/obligations", json=_payload(obligation_type="paye"), headers=H1).json()["id"]
    client.post(f"/obligations/{oid}/file", params={"company_id": "co-tc", "filed_amount": 500}, headers=H1)
    summary = client.get("/summary", params={"company_id": "co-tc"}, headers=H1).json()
    assert summary["total_obligations"] == 2
    assert summary["filed"] == 1
    assert summary["pending"] == 1
    assert summary["compliance_score"] == 50.0
    assert summary["obligations"][0]["status"] in ("pending", "filed")
    # empty company scores 100
    empty = client.get("/summary", params={"company_id": "no-such-co"}, headers=H1).json()
    assert empty["compliance_score"] == 100
    # other user sees nothing of it
    other = client.get("/summary", params={"company_id": "co-tc"}, headers=H2).json()
    assert other["total_obligations"] == 0


def test_book_a_b_isolation():
    client.post("/obligations", json=_payload(description="book-a"), headers=H1)
    client.post(
        "/obligations",
        json=_payload(description="book-b"),
        headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
    )
    assert len(client.get("/obligations", params={"company_id": "co-tc"}, headers=H1).json()) == 1
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert len(client.get("/obligations", params={"company_id": "co-tc"}, headers=other_book).json()) == 1
    # personal view spans both Books
    assert len(client.get("/obligations", params={"company_id": "co-tc"}, headers=H1_PERSONAL).json()) == 2
    # summaries are per-Book
    assert client.get("/summary", params={"company_id": "co-tc"}, headers=H1).json()["total_obligations"] == 1
    assert client.get("/summary", params={"company_id": "co-tc"}, headers=other_book).json()["total_obligations"] == 1
    assert client.get("/summary", params={"company_id": "co-tc"}, headers=H1_PERSONAL).json()["total_obligations"] == 2


def test_x_user_id_required():
    assert client.post("/obligations", json=_payload()).status_code in (401, 403, 422)
    assert client.get("/obligations", params={"company_id": "co-tc"}).status_code in (401, 403, 422)
    assert client.get("/summary", params={"company_id": "co-tc"}).status_code in (401, 403, 422)
