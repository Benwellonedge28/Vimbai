"""Book-scoping and persistence tests for appropriation-control-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from appropriation_control_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ac_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "ac-user-1", "ac-user-2"
BOOK_A, BOOK_B = "ac-book-a", "ac-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _appr(company="co-ac", department="IT", approved=100000.0):
    return {"company_id": company, "department": department, "fiscal_year": "2026", "approved_amount": approved}


def test_create_and_list_appropriations():
    resp = client.post("/appropriations", json=_appr(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["available_amount"] == 100000.0
    assert data["book_id"] == BOOK_A
    assert data["status"] == "active"

    listed = client.get("/appropriations/co-ac", headers=H1).json()
    assert listed["total"] == 1
    assert listed["appropriations"][0]["department"] == "IT"

    # department filter + user isolation
    assert client.get("/appropriations/co-ac", params={"department": "IT"}, headers=H1).json()["total"] == 1
    assert client.get("/appropriations/co-ac", params={"department": "HR"}, headers=H1).json()["total"] == 0
    assert client.get("/appropriations/co-ac", headers=H2).json()["total"] == 0


def test_transaction_lifecycle_persists():
    appr_id = client.post("/appropriations", json=_appr(), headers=H1).json()["id"]

    commit = client.post(
        "/transactions", json={"appropriation_id": appr_id, "type": "commit", "amount": 30000}, headers=H1
    )
    assert commit.json()["available"] == 70000.0

    spend = client.post(
        "/transactions", json={"appropriation_id": appr_id, "type": "spend", "amount": 30000}, headers=H1
    )
    assert spend.json()["available"] == 70000.0

    refund = client.post(
        "/transactions", json={"appropriation_id": appr_id, "type": "refund", "amount": 5000}, headers=H1
    )
    assert refund.json()["available"] == 75000.0

    uncommit = client.post(
        "/transactions", json={"appropriation_id": appr_id, "type": "uncommit", "amount": 2000}, headers=H1
    )
    assert uncommit.json()["available"] == 77000.0

    # aggregates persisted on the node
    stored = client.get("/appropriations/co-ac", headers=H1).json()["appropriations"][0]
    assert stored["committed_amount"] == -2000.0
    assert stored["spent_amount"] == 25000.0
    assert stored["available_amount"] == 77000.0

    check = client.get(f"/check/{appr_id}", params={"amount": 80000}, headers=H1).json()
    assert check["allowed"] is False
    check_ok = client.get(f"/check/{appr_id}", params={"amount": 70000}, headers=H1).json()
    assert check_ok["allowed"] is True


def test_exhaustion_status():
    appr_id = client.post("/appropriations", json=_appr(approved=10000.0), headers=H1).json()["id"]
    # spend assumes a prior commit (original semantics preserved)
    client.post("/transactions", json={"appropriation_id": appr_id, "type": "commit", "amount": 10000.0}, headers=H1)
    resp = client.post(
        "/transactions", json={"appropriation_id": appr_id, "type": "spend", "amount": 10000.0}, headers=H1
    )
    assert resp.json()["available"] == 0.0
    assert resp.json()["status"] == "exhausted"
    stored = client.get("/appropriations/co-ac", headers=H1).json()["appropriations"][0]
    assert stored["status"] == "exhausted"


def test_cross_user_and_cross_book_404():
    appr_id = client.post("/appropriations", json=_appr(), headers=H1).json()["id"]
    # other user cannot transact or check
    blocked_tx = client.post(
        "/transactions", json={"appropriation_id": appr_id, "type": "commit", "amount": 100}, headers=H2
    )
    assert blocked_tx.status_code == 404
    assert client.get(f"/check/{appr_id}", params={"amount": 10}, headers=H2).status_code == 404
    # other Book same user cannot either
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    blocked_tx2 = client.post(
        "/transactions", json={"appropriation_id": appr_id, "type": "commit", "amount": 100}, headers=other_book
    )
    assert blocked_tx2.status_code == 404
    assert client.get(f"/check/{appr_id}", params={"amount": 10}, headers=other_book).status_code == 404
    # original untouched
    stored = client.get("/appropriations/co-ac", headers=H1).json()["appropriations"][0]
    assert stored["available_amount"] == 100000.0


def test_book_a_b_isolation():
    client.post("/appropriations", json=_appr(company="co-a"), headers=H1)
    client.post("/appropriations", json=_appr(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/appropriations/co-a", headers=H1).json()["total"] == 1
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/appropriations/co-a", headers=other_book).json()["total"] == 0
    # personal spans books
    assert client.get("/appropriations/co-a", headers=H1_PERSONAL).json()["total"] == 1
    assert client.get("/appropriations/co-b", headers=H1_PERSONAL).json()["total"] == 1


def test_unknown_tx_type_404():
    appr_id = client.post("/appropriations", json=_appr(), headers=H1).json()["id"]
    resp = client.post("/transactions", json={"appropriation_id": appr_id, "type": "steal", "amount": 100}, headers=H1)
    assert resp.status_code == 404


def test_x_user_id_required():
    assert client.post("/appropriations", json=_appr()).status_code in (401, 403, 422)
    assert client.get("/appropriations/co-ac").status_code in (401, 403, 422)
