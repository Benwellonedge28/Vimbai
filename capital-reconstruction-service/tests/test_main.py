"""Book-scoping and persistence tests for capital-reconstruction-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from capital_reconstruction_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("cr_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "cr-user-1", "cr-user-2"
BOOK_A, BOOK_B = "cr-book-a", "cr-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_reconstruction(client, headers, **kw):
    params = {
        "company_id": "co-1",
        "reconstruction_type": "write_off_excess_capital",
        "description": "Reduce capital by 40k",
        "scheme_date": "2026-01-15",
        "previous_share_capital": 150000.0,
        "new_share_capital": 110000.0,
    }
    params.update(kw)
    return client.post("/reconstructions/create", params=params, headers=headers)


def test_health():
    assert client.get("/health").json()["status"] == "healthy"


def test_create_and_isolation():
    r = _mk_reconstruction(client, H1)
    assert r.status_code == 200, r.text
    rec = r.json()
    assert rec["capital_reduction_amount"] == 40000.0
    assert rec["status"] == "draft"

    _mk_reconstruction(client, H2, company_id="co-2")
    _mk_reconstruction(client, {"X-User-Id": U1, "X-Book-ID": BOOK_B}, company_id="co-3")

    assert len(client.get("/reconstructions", headers=H1).json()["reconstructions"]) == 1
    assert len(client.get("/reconstructions", headers=H2).json()["reconstructions"]) == 1
    assert len(client.get("/reconstructions", params={"company_id": "co-1"}, headers=H1).json()["reconstructions"]) == 1
    assert len(client.get("/reconstructions", params={"company_id": "co-2"}, headers=H1).json()["reconstructions"]) == 0

    # cross-scope get behaves as miss (original contract)
    assert client.get(f"/reconstructions/{rec['id']}", headers=H2).json() == {"error": "Reconstruction not found"}


def test_adjustments_and_conversions_scoped_to_parent_owner():
    rec = _mk_reconstruction(client, H1).json()

    # cross-user cannot attach children (parent invisible)
    r2 = client.post(
        f"/reconstructions/{rec['id']}/adjustments/add",
        params={
            "account_code": "3200",
            "account_name": "Share Capital",
            "previous_balance": 150000.0,
            "adjustment_type": "write_off",
            "adjustment_amount": 40000.0,
            "description": "Write off excess",
        },
        headers=H2,
    )
    assert r2.json() == {"error": "Reconstruction not found"}

    # owner attaches fine
    r = client.post(
        f"/reconstructions/{rec['id']}/adjustments/add",
        params={
            "account_code": "3200",
            "account_name": "Share Capital",
            "previous_balance": 150000.0,
            "adjustment_type": "transfer",
            "adjustment_amount": -40000.0,
            "description": "Transfer to reserves",
        },
        headers=H1,
    )
    assert r.status_code == 200, r.text
    adj = r.json()
    assert adj["new_balance"] == 110000.0  # transfer: prev + amount

    conv = client.post(
        f"/reconstructions/{rec['id']}/reserve-conversions/add",
        params={
            "from_account": "3300",
            "from_account_name": "Retained Earnings",
            "to_account": "3310",
            "to_account_name": "Capital Redemption Reserve",
            "amount": 15000.0,
            "reason": "Statutory conversion",
        },
        headers=H1,
    )
    assert conv.status_code == 200, r.text
    assert conv.json()["amount"] == 15000.0

    # details include children for owner, nothing for other user
    detail = client.get(f"/reconstructions/{rec['id']}", headers=H1).json()
    assert len(detail["adjustments"]) == 1
    assert len(detail["reserve_conversions"]) == 1
    assert client.get(f"/reconstructions/{rec['id']}", headers=H2).json() == {"error": "Reconstruction not found"}


def test_approve_completes_and_persists():
    rec = _mk_reconstruction(client, H1).json()
    client.post(
        f"/reconstructions/{rec['id']}/adjustments/add",
        params={
            "account_code": "3500",
            "account_name": "Preliminary Expenses",
            "previous_balance": 5000.0,
            "adjustment_type": "write_off",
            "adjustment_amount": 5000.0,
            "description": "Write off",
        },
        headers=H1,
    )

    r = client.post(
        f"/reconstructions/{rec['id']}/approve",
        params={"court_approval_date": "2026-01-20", "shareholders_approval_date": "2026-01-22"},
        headers=H1,
    )
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["reconstruction"]["status"] == "completed"
    assert body["reconstruction"]["court_approval_date"] is not None
    assert len(body["adjustments"]) == 1
    assert body["conversions"] == []

    # persisted status survives re-read
    detail = client.get(f"/reconstructions/{rec['id']}", headers=H1).json()
    assert detail["reconstruction"]["status"] == "completed"

    # cross-scope approve: miss contract
    rec2 = _mk_reconstruction(client, H2, company_id="co-9").json()
    assert client.post(f"/reconstructions/{rec2['id']}/approve", headers=H1).json() == {
        "error": "Reconstruction not found"
    }


def test_summary_scoped():
    _mk_reconstruction(client, H1)
    _mk_reconstruction(client, H2, company_id="co-1")
    s = client.get("/summary/co-1", headers=H1).json()
    assert s["total_reconstructions"] == 1
    assert s["total_capital_reduced"] == 40000.0
    assert s["completed_reconstructions"] == 0
