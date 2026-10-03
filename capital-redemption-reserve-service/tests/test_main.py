"""Book-scoping and persistence tests for capital-redemption-reserve-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from capital_redemption_reserve_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("crr_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "crr-user-1", "crr-user-2"
BOOK_A, BOOK_B = "crr-book-a", "crr-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def test_health():
    assert client.get("/health").json()["status"] == "healthy"


def test_redemption_record_and_isolation():
    r = client.post(
        "/redemptions/record",
        params={
            "company_id": "co-1",
            "share_class": "preference",
            "shares_redeemed": 10000,
            "redemption_price": 1.50,
            "nominal_value": 1.00,
            "redemption_date": "2026-02-01",
            "source_account": "proceeds",
        },
        headers=H1,
    )
    assert r.status_code == 200, r.text
    tx = r.json()
    assert tx["total_proceeds"] == 15000.0
    assert tx["redemption_reserve_amount"] == 5000.0
    assert tx["status"] == "completed"

    # persisted + scoped
    listed = client.get("/redemptions", headers=H1).json()["redemptions"]
    assert len(listed) == 1
    assert listed[0]["redemption_reserve_amount"] == 5000.0
    assert client.get("/redemptions", headers=H2).json()["redemptions"] == []

    # company filter
    assert len(client.get("/redemptions", params={"company_id": "co-1"}, headers=H1).json()["redemptions"]) == 1
    assert len(client.get("/redemptions", params={"company_id": "co-9"}, headers=H1).json()["redemptions"]) == 0


def test_creation_and_utilization():
    c = client.post(
        "/creations/create",
        params={
            "company_id": "co-1",
            "amount": 8000.0,
            "source": "capital_reduction",
            "description": "From reduction scheme",
        },
        headers=H1,
    )
    assert c.status_code == 200, c.text
    assert c.json()["amount"] == 8000.0
    assert client.get("/creations", headers=H1).json()["creations"][0]["source"] == "capital_reduction"

    u = client.post(
        "/utilizations/record",
        params={
            "company_id": "co-1",
            "amount": 3000.0,
            "utilization_type": "bonus_issue",
            "description": "Bonus to ordinary holders",
        },
        headers=H1,
    )
    assert u.status_code == 200, u.text
    assert u.json()["utilization_type"] == "bonus_issue"
    assert len(client.get("/utilizations", headers=H1).json()["utilizations"]) == 1
    assert client.get("/utilizations", headers=H2).json()["utilizations"] == []


def test_summary_scoped_to_caller():
    # U1 records for co-1
    client.post(
        "/redemptions/record",
        params={
            "company_id": "co-1",
            "share_class": "ordinary",
            "shares_redeemed": 5000,
            "redemption_price": 2.0,
            "nominal_value": 1.0,
            "redemption_date": "2026-03-01",
            "source_account": "fresh_issue",
        },
        headers=H1,
    )
    # U2 records for the same company id — must not leak into U1's summary
    client.post(
        "/creations/create",
        params={"company_id": "co-1", "amount": 99000.0, "source": "capital_reduction", "description": "foreign"},
        headers=H2,
    )
    s = client.get("/summary/co-1", headers=H1).json()
    assert s["total_created"] == 5000.0  # only U1's redemption reserve
    assert s["total_utilized"] == 0.0
    assert s["current_balance"] == 5000.0
    assert s["transaction_count"] == 1

    s2 = client.get("/summary/co-1", headers=H2).json()
    assert s2["total_created"] == 99000.0
    assert s2["transaction_count"] == 1

    # Book-gated: same user, different Book, sees nothing
    s3 = client.get("/summary/co-1", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()
    assert s3["transaction_count"] == 0
    assert s3["current_balance"] == 0.0
