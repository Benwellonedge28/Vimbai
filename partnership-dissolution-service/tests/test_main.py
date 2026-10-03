"""Book-scoping and persistence tests for partnership-dissolution-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from partnership_dissolution_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("pd_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "pd-user-1", "pd-user-2"
BOOK_A, BOOK_B = "pd-book-a", "pd-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


ASSETS = [
    {"asset_id": "A1", "asset_name": "Truck", "book_value": 30000.0, "sale_proceeds": 35000.0},
    {"asset_id": "A2", "asset_name": "Shelves", "book_value": 8000.0, "sale_proceeds": 5000.0},
]
CREDITORS = [{"id": "C1", "name": "Supplier", "amount": 4000.0, "paid": 3800.0, "discount": 200.0}]
PARTNERS = [
    {"id": "P1", "name": "Tendai", "capital": 40000.0, "current": 2000.0, "share": 3000.0},
    {"id": "P2", "name": "Chipo", "capital": 20000.0, "current": 500.0, "share": -500.0},
]


def _mk_dissolution(client, headers, **kw):
    params = {
        "partnership_id": "ps-1",
        "dissolution_date": "2026-06-01",
        "reason": "mutual_agreement",
    }
    params.update(kw)
    return client.post(
        "/dissolve",
        params=params,
        json={"assets": ASSETS, "creditors": CREDITORS, "partners": PARTNERS},
        headers=headers,
    )


def test_health():
    assert client.get("/health").json()["status"] == "healthy"


def test_dissolve_math_and_persistence():
    r = _mk_dissolution(client, H1)
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["status"] == "completed"
    assert body["total_assets_realized"] == 40000.0
    assert body["realization_profit"] == 5000.0
    assert body["realization_loss"] == 3000.0
    assert body["total_creditors"] == 3800.0
    assert body["total_partners_capitals"] == 65000.0  # 45000 + 20000
    assert len(body["assets"]) == 2 and len(body["partners"]) == 2

    # persisted and caller-scoped
    listed = client.get("/dissolutions", headers=H1).json()["dissolutions"]
    assert len(listed) == 1
    assert listed[0]["realization_profit"] == 5000.0
    assert client.get("/dissolutions", headers=H2).json()["dissolutions"] == []


def test_isolation_and_lookup_miss():
    r1 = _mk_dissolution(client, H1)
    d1 = r1.json()
    _mk_dissolution(client, H2, partnership_id="ps-2")
    _mk_dissolution(client, {"X-User-Id": U1, "X-Book-ID": BOOK_B}, partnership_id="ps-3")

    # each caller sees only their own Book-visible records
    assert len(client.get("/dissolutions", headers=H1).json()["dissolutions"]) == 1
    assert len(client.get("/dissolutions", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()["dissolutions"]) == 1
    assert len(client.get("/dissolutions", headers=H2).json()["dissolutions"]) == 1

    # cross-scope lookup behaves as not-found (original miss contract kept)
    assert client.get(f"/dissolutions/{d1['id']}", headers=H2).json() == {"error": "Not found"}
    assert client.get(f"/dissolutions/{d1['id']}", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json() == {
        "error": "Not found"
    }
    assert client.get(f"/dissolutions/{d1['id']}", headers=H1).json()["id"] == d1["id"]
    assert client.get("/dissolutions/none", headers=H1).json() == {"error": "Not found"}
