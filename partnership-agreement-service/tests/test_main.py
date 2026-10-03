"""Book-scoping and persistence tests for partnership-agreement-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from partnership_agreement_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("pa_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "pa-user-1", "pa-user-2"
BOOK_A, BOOK_B = "pa-book-a", "pa-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_agreement(client, headers, **kw):
    payload = {
        "agreement_number": "PA-001",
        "partnership_name": "Mukuru Traders",
        "partners": [
            {"name": "Tendai", "address": "Harare", "contribution": 50000.0, "profit_sharing_ratio": 0.6},
            {"name": "Chipo", "address": "Bulawayo", "contribution": 30000.0, "profit_sharing_ratio": 0.4},
        ],
        "start_date": "2026-01-01",
        "business_nature": "retail",
    }
    payload.update(kw)
    return client.post("/agreements", json=payload, headers=headers)


def test_health_and_create():
    assert client.get("/health").json()["status"] == "healthy"
    r = _mk_agreement(client, H1)
    assert r.status_code == 201, r.text
    agg = r.json()
    assert agg["capital_amount"] == 80000.0
    assert len(agg["partners"]) == 2


def test_isolation_and_get():
    agg = _mk_agreement(client, H1).json()
    other = _mk_agreement(client, H2).json()
    other_book = _mk_agreement(client, {"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()

    # caller sees only their own
    mine = client.get("/agreements", headers=H1).json()
    assert mine["count"] == 1
    assert mine["agreements"][0]["id"] == agg["id"]

    # cross-user / cross-Book get: 404
    assert client.get(f"/agreements/{agg['id']}", headers=H2).status_code == 404
    assert client.get(f"/agreements/{agg['id']}", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).status_code == 404
    assert client.get(f"/agreements/{other['id']}", headers=H1).status_code == 404
    assert client.get("/agreements/none", headers=H1).status_code == 404


def test_update_setattr_semantics():
    agg = _mk_agreement(client, H1).json()
    r = client.put(f"/agreements/{agg['id']}", json={"business_nature": "wholesale"}, headers=H1)
    assert r.status_code == 200, r.text
    assert r.json()["business_nature"] == "wholesale"
    # unchanged field survives
    assert r.json()["partnership_name"] == "Mukuru Traders"
    # unknown key ignored
    r2 = client.put(f"/agreements/{agg['id']}", json={"nonexistent_field": 1}, headers=H1)
    assert r2.status_code == 200
    # cross-user update: 404
    assert client.put(f"/agreements/{agg['id']}", json={"business_nature": "x"}, headers=H2).status_code == 404


def test_add_partner():
    agg = _mk_agreement(client, H1).json()
    r = client.post(
        f"/agreements/{agg['id']}/partners/add",
        json={"name": "Rudo", "address": "Gweru", "contribution": 20000.0, "profit_sharing_ratio": 0.1},
        headers=H1,
    )
    assert r.status_code == 200, r.text
    body = r.json()
    assert len(body["partners"]) == 3
    assert body["capital_amount"] == 100000.0

    # persisted
    got = client.get(f"/agreements/{agg['id']}", headers=H1).json()
    assert len(got["partners"]) == 3
    assert got["capital_amount"] == 100000.0

    # cross-user add: 404
    assert (
        client.post(
            f"/agreements/{agg['id']}/partners/add",
            json={"name": "X", "address": "y", "contribution": 1.0, "profit_sharing_ratio": 0.1},
            headers=H2,
        ).status_code
        == 404
    )


def test_summary_scoped():
    agg = _mk_agreement(client, H1).json()
    _mk_agreement(client, H2, partnership_name="Foreign Firm")

    s = client.get(f"/agreements/{agg['id']}/summary", headers=H1).json()
    assert s["partnership_name"] == "Mukuru Traders"
    assert s["total_capital"] == 80000.0
    assert s["partner_count"] == 2
    assert {p["name"] for p in s["partners"]} == {"Tendai", "Chipo"}
    assert client.get(f"/agreements/{agg['id']}/summary", headers=H2).status_code == 404


def test_is_active_filter():
    _mk_agreement(client, H1)
    _mk_agreement(client, H1, agreement_number="PA-002", partnership_name="Dormant")
    agg_id = client.get("/agreements", headers=H1).json()["agreements"][1]["id"]
    client.put(f"/agreements/{agg_id}", json={"is_active": False}, headers=H1)

    assert client.get("/agreements", params={"is_active": True}, headers=H1).json()["count"] == 1
    assert client.get("/agreements", params={"is_active": False}, headers=H1).json()["count"] == 1
    assert client.get("/agreements", headers=H1).json()["count"] == 2
