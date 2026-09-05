"""Book-scoping and persistence tests for trade-finance-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from trade_finance_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("tf_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "tf-user-1", "tf-user-2"
BOOK_A, BOOK_B = "tf-book-a", "tf-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_inst(company="co-tf", itype="letter_of_credit", amount=200000.0):
    return {
        "company_id": company,
        "instrument_type": itype,
        "counterparty": "Overseas Supplier",
        "amount": amount,
        "currency": "USD",
        "issuing_bank": "Stanbic",
    }


def test_create_instrument_fee_and_risk():
    resp = client.post("/instruments", json=_mk_inst(), headers=H1)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["fee_estimate"] == 400.0  # 200000 * 0.002
    assert body["risk_assessment"] == "medium"
    assert "LC application" in body["documentation_required"]


def test_instruments_persist_with_status_filter():
    i1 = client.post("/instruments", json=_mk_inst(company="co-tf"), headers=H1).json()
    i2 = client.post(
        "/instruments", json=_mk_inst(company="co-tf", itype="factoring", amount=50000.0), headers=H1
    ).json()
    client.post(f"/instruments/{i2['id']}/present", params={"company_id": "co-tf"}, headers=H1)

    listed = client.get("/instruments", params={"company_id": "co-tf"}, headers=H1).json()
    assert len(listed) == 2
    presented = client.get("/instruments", params={"company_id": "co-tf", "status": "presented"}, headers=H1).json()
    assert len(presented) == 1
    assert presented[0]["instrument_type"] == "factoring"


def test_user_isolation():
    client.post("/instruments", json=_mk_inst(), headers=H1)
    assert client.get("/instruments", params={"company_id": "co-tf"}, headers=H2).json() == []


def test_book_a_b_isolation():
    client.post("/instruments", json=_mk_inst(), headers=H1)
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/instruments", params={"company_id": "co-tf"}, headers=other).json() == []


def test_personal_view_spans_books():
    client.post("/instruments", json=_mk_inst(company="co-a"), headers=H1)
    client.post("/instruments", json=_mk_inst(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert len(client.get("/instruments", params={"company_id": "co-a"}, headers=H1_PERSONAL).json()) == 1
    assert len(client.get("/instruments", params={"company_id": "co-b"}, headers=H1_PERSONAL).json()) == 1


def test_present_settle_lifecycle_and_scoping():
    inst = client.post("/instruments", json=_mk_inst(), headers=H1).json()
    presented = client.post(f"/instruments/{inst['id']}/present", params={"company_id": "co-tf"}, headers=H1)
    assert presented.json()["status"] == "presented"
    settled = client.post(f"/instruments/{inst['id']}/settle", params={"company_id": "co-tf"}, headers=H1)
    assert settled.json()["status"] == "paid"
    assert settled.json()["amount"] == 200000.0

    # status persisted
    paid = client.get("/instruments", params={"company_id": "co-tf", "status": "paid"}, headers=H1).json()
    assert len(paid) == 1


def test_cross_user_settle_404():
    inst = client.post("/instruments", json=_mk_inst(), headers=H1).json()
    assert (
        client.post(f"/instruments/{inst['id']}/present", params={"company_id": "co-tf"}, headers=H2).status_code == 404
    )


def test_cross_book_settle_404():
    inst = client.post("/instruments", json=_mk_inst(), headers=H1).json()
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert (
        client.post(f"/instruments/{inst['id']}/settle", params={"company_id": "co-tf"}, headers=other).status_code
        == 404
    )
    # original Book still untouched
    listed = client.get("/instruments", params={"company_id": "co-tf"}, headers=H1).json()
    assert listed[0]["status"] == "issued"


def test_x_user_id_required():
    assert client.post("/instruments", json=_mk_inst()).status_code in (401, 403, 422)
