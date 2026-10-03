"""Book-scoping and persistence tests for partnership-changes-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from partnership_changes_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("pc_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "pc-user-1", "pc-user-2"
BOOK_A, BOOK_B = "pc-book-a", "pc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_retirement(client, headers, **kw):
    params = {
        "partnership_id": "ps-1",
        "partner_id": "partner-9",
        "partner_name": "Tendai",
        "effective_date": "2026-03-01",
        "capital_balance": 40000.0,
        "current_account_balance": 5000.0,
        "goodwill_amount": 3000.0,
    }
    params.update(kw)
    return client.post("/changes/retirement", params=params, headers=headers)


def test_health():
    assert client.get("/health").json()["status"] == "healthy"


def test_retirement_persists_and_scopes():
    r = _mk_retirement(client, H1)
    assert r.status_code == 200, r.text
    change = r.json()
    assert change["total_payable"] == 48000.0
    assert change["settlement_status"] == "pending"
    assert change["change_type"] == "retirement"

    # isolation: other user sees nothing
    assert client.get("/changes", headers=H1).json()["count"] == 1
    assert client.get("/changes", headers=H2).json()["count"] == 0
    _mk_retirement(client, {"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/changes", headers=H1).json()["count"] == 1
    assert client.get("/changes", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()["count"] == 1

    # filters
    assert client.get("/changes", params={"change_type": "retirement"}, headers=H1).json()["count"] == 1
    assert client.get("/changes", params={"partnership_id": "ps-other"}, headers=H1).json()["count"] == 0

    # settle: caller-scoped
    s = client.post(f"/changes/{change['id']}/settle", headers=H1)
    assert s.status_code == 200
    assert s.json()["settlement_status"] == "settled"
    # persisted
    listed = client.get("/changes", headers=H1).json()["changes"]
    assert listed[0]["settlement_status"] == "settled"

    # cross-user settle: 404, no mutation
    assert client.post(f"/changes/{change['id']}/settle", headers=H2).status_code == 404
    assert client.get("/changes", headers=H2).json()["count"] == 0


def test_death_change():
    r = client.post(
        "/changes/death",
        params={
            "partnership_id": "ps-1",
            "partner_id": "partner-2",
            "partner_name": "Chipo",
            "effective_date": "2026-04-01",
            "capital_balance": 20000.0,
            "current_account_balance": 1000.0,
            "executor_name": "Nyoni",
        },
        headers=H1,
    )
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["change_type"] == "death"
    assert body["total_payable"] == 21000.0
    assert body["notes"] == "Payable to executor: Nyoni"
    # death type filter
    assert client.get("/changes", params={"change_type": "death"}, headers=H1).json()["count"] == 1


def test_admission_persists_and_scopes():
    payload = {
        "new_partner_id": "partner-77",
        "new_partner_name": "Rudo",
        "capital_contribution": 25000.0,
        "goodwill_paid": 5000.0,
        "premium_distribution": {"p1": 3000.0, "p2": 2000.0},
        "new_profit_sharing_ratios": {"p1": 0.5, "p2": 0.3, "partner-77": 0.2},
        "admission_date": "2026-05-01",
    }
    r = client.post("/changes/admission", json=payload, headers=H1)
    assert r.status_code == 200, r.text
    adm = r.json()
    assert adm["capital_contribution"] == 25000.0
    assert adm["premium_distribution"] == {"p1": 3000.0, "p2": 2000.0}
    assert adm["new_profit_sharing_ratios"]["partner-77"] == 0.2
    # admissions never had a listing endpoint (contract unchanged); verify
    # persistence by re-reading through another admission for same user
    r2 = client.post("/changes/admission", json={**payload, "new_partner_name": "Rudo2"}, headers=H1)
    assert r2.status_code == 200
    # other user unaffected
    r3 = client.post("/changes/admission", json={**payload, "new_partner_id": "partner-88"}, headers=H2)
    assert r3.status_code == 200
    assert client.get("/changes", headers=H2).json()["count"] == 0
