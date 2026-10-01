"""Book-scoping and persistence tests for revenue-recognition-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from revenue_recognition_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("rr_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "rr-user-1", "rr-user-2"
BOOK_A, BOOK_B = "rr-book-a", "rr-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _contract(company="co-rr", customer="Customer A", price=100000.0):
    return {
        "company_id": company,
        "customer_name": customer,
        "obligations": [
            {
                "description": "Software License",
                "transaction_price": price * 0.8,
                "standalone_selling_price": price * 0.8,
                "recognition_method": "point_in_time",
            },
            {
                "description": "Implementation",
                "transaction_price": price * 0.2,
                "standalone_selling_price": price * 0.2,
                "recognition_method": "over_time",
            },
        ],
    }


def test_create_contract_allocates_price():
    resp = client.post("/contracts", json=_contract(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["total_transaction_price"] == 100000.0
    assert data["book_id"] == BOOK_A
    assert len(data["obligations"]) == 2
    assert data["obligations"][0]["transaction_price"] == 80000.0


def test_recognize_persists_and_scopes():
    contract = client.post("/contracts", json=_contract(), headers=H1).json()
    oid = contract["obligations"][0]["id"]

    recog = client.post(
        f"/contracts/{contract['id']}/recognize",
        params={"obligation_id": oid, "amount": 80000},
        headers=H1,
    )
    assert recog.status_code == 200, recog.text
    body = recog.json()
    assert body["is_satisfied"] is True
    assert body["contract_total_recognized"] == 80000.0
    assert body["deferred"] == 20000.0

    # recognition persisted on the contract node
    stored = client.get("/contracts/co-rr", headers=H1).json()
    assert stored["contracts"][0]["total_revenue_recognized"] == 80000.0
    assert stored["contracts"][0]["obligations"][0]["is_satisfied"] is True

    # other user cannot see or recognize the contract
    assert client.get("/contracts/co-rr", headers=H2).json()["total"] == 0
    blocked = client.post(
        f"/contracts/{contract['id']}/recognize",
        params={"obligation_id": oid, "amount": 1000},
        headers=H2,
    )
    assert blocked.status_code == 404


def test_cross_book_recognize_404_and_isolation():
    contract = client.post("/contracts", json=_contract(), headers=H1).json()
    assert client.get("/contracts/co-rr", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()["total"] == 0
    oid = contract["obligations"][1]["id"]
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    blocked = client.post(
        f"/contracts/{contract['id']}/recognize",
        params={"obligation_id": oid, "amount": 5000},
        headers=other,
    )
    assert blocked.status_code == 404
    # original Book untouched
    stored = client.get("/contracts/co-rr", headers=H1).json()
    assert stored["contracts"][0]["total_revenue_recognized"] == 0


def test_personal_view_spans_books():
    client.post("/contracts", json=_contract(company="co-a"), headers=H1)
    client.post("/contracts", json=_contract(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/contracts/co-a", headers=H1_PERSONAL).json()["total"] == 1
    assert client.get("/contracts/co-b", headers=H1_PERSONAL).json()["total"] == 1
    assert client.get("/contracts/co-a", headers=H1).json()["total"] == 1


def test_summary_scoped():
    client.post("/contracts", json=_contract(), headers=H1)
    s = client.get("/summary/co-rr", headers=H1).json()
    assert s["total_contracts"] == 1
    assert s["total_contract_value"] == 100000.0
    assert s["deferred_revenue"] == 100000.0
    other = client.get("/summary/co-rr", headers=H2).json()
    assert other["total_contracts"] == 0
    assert other["total_contract_value"] == 0


def test_recognize_default_full_price():
    contract = client.post("/contracts", json=_contract(), headers=H1).json()
    oid = contract["obligations"][1]["id"]
    recog = client.post(f"/contracts/{contract['id']}/recognize", params={"obligation_id": oid}, headers=H1)
    assert recog.json()["recognized"] == 20000.0
    assert recog.json()["is_satisfied"] is True


def test_x_user_id_required():
    assert client.post("/contracts", json=_contract()).status_code in (401, 403, 422)
