"""Book-scoping and persistence tests for insurance-claims-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from insurance_claims_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ic_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "ic-user-1", "ic-user-2"
BOOK_A, BOOK_B = "ic-book-a", "ic-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _claim(company="co-ic", amount=50000.0, **kw):
    payload = {
        "company_id": company,
        "policy_number": "POL-001",
        "claim_type": "property",
        "incident_date": "2026-06-15",
        "claim_amount": amount,
        "deductible": 5000,
        "coverage_limit": 100000,
        "description": "Warehouse fire",
    }
    payload.update(kw)
    return payload


def test_file_claim_persists():
    resp = client.post("/file", json=_claim(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["status"] == "filed"
    assert data["book_id"] == BOOK_A
    listed = client.get("/claims", params={"company_id": "co-ic"}, headers=H1).json()
    assert len(listed) == 1
    assert listed[0]["policy_number"] == "POL-001"


def test_list_status_filter():
    client.post("/file", json=_claim(), headers=H1)
    assert client.get("/claims", params={"company_id": "co-ic", "status": "filed"}, headers=H1).json()
    assert client.get("/claims", params={"company_id": "co-ic", "status": "approved"}, headers=H1).json() == []


def test_user_isolation():
    client.post("/file", json=_claim(), headers=H1)
    assert client.get("/claims", params={"company_id": "co-ic"}, headers=H2).json() == []


def test_book_a_b_isolation():
    client.post("/file", json=_claim(company="co-a"), headers=H1)
    client.post("/file", json=_claim(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert len(client.get("/claims", params={"company_id": "co-a"}, headers=H1).json()) == 1
    assert (
        client.get("/claims", params={"company_id": "co-a"}, headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()
        == []
    )
    # personal spans books
    assert len(client.get("/claims", params={"company_id": "co-a"}, headers=H1_PERSONAL).json()) == 1
    assert len(client.get("/claims", params={"company_id": "co-b"}, headers=H1_PERSONAL).json()) == 1


def test_process_claim_persists_status():
    claim = client.post("/file", json=_claim(), headers=H1).json()
    result = client.post(f"/claims/{claim['id']}/process", params={"company_id": "co-ic"}, headers=H1)
    assert result.status_code == 200, result.text
    data = result.json()
    assert data["covered_amount"] == 45000.0
    assert data["status"] == "approved"
    assert data["coverage_ratio"] == 0.9

    # status persisted on the node
    stored = client.get("/claims", params={"company_id": "co-ic"}, headers=H1).json()
    assert stored[0]["status"] == "approved"


def test_process_claim_denied_when_deductible_exceeds():
    claim = client.post("/file", json=_claim(amount=3000.0, deductible=5000.0), headers=H1).json()
    result = client.post(f"/claims/{claim['id']}/process", params={"company_id": "co-ic"}, headers=H1)
    assert result.json()["status"] == "denied"
    assert result.json()["settlement_amount"] == 0


def test_process_cross_user_404():
    claim = client.post("/file", json=_claim(), headers=H1).json()
    blocked = client.post(f"/claims/{claim['id']}/process", params={"company_id": "co-ic"}, headers=H2)
    assert blocked.status_code == 404


def test_process_cross_book_404():
    claim = client.post("/file", json=_claim(), headers=H1).json()
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    blocked = client.post(f"/claims/{claim['id']}/process", params={"company_id": "co-ic"}, headers=other)
    assert blocked.status_code == 404
    # original untouched
    stored = client.get("/claims", params={"company_id": "co-ic"}, headers=H1).json()
    assert stored[0]["status"] == "filed"


def test_process_wrong_company_404():
    claim = client.post("/file", json=_claim(company="co-x"), headers=H1).json()
    blocked = client.post(f"/claims/{claim['id']}/process", params={"company_id": "co-other"}, headers=H1)
    assert blocked.status_code == 404


def test_x_user_id_required():
    assert client.post("/file", json=_claim()).status_code in (401, 403, 422)
