"""Book-scoping and persistence tests for fixed-assets-register-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from fixed_assets_register_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("far_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "far-user-1", "far-user-2"
BOOK_A, BOOK_B = "far-book-a", "far-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_asset(client, headers, **kw):
    params = {
        "asset_code": "AST-1",
        "asset_name": "Tractor",
        "category": "machinery",
        "acquisition_date": "2026-01-01",
        "acquisition_cost": 12000.0,
        "useful_life_years": 10,
    }
    params.update(kw)
    return client.post("/assets", params=params, headers=headers)


def test_health():
    for path in ("/", "/health"):
        assert client.get(path).json()["status"] == "healthy"


def test_register_and_get_asset():
    resp = _mk_asset(client, H1)
    assert resp.status_code == 200, resp.text
    asset = resp.json()
    assert asset["net_book_value"] == 12000.0
    assert asset["status"] == "active"

    # invalid category: 400
    assert _mk_asset(client, H1, category="magic").status_code == 400

    got = client.get(f"/assets/{asset['id']}", headers=H1).json()
    assert got["id"] == asset["id"]

    # cross-user / cross-Book: 404, no existence leak
    assert client.get(f"/assets/{asset['id']}", headers=H2).status_code == 404
    assert client.get(f"/assets/{asset['id']}", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).status_code == 404
    assert client.get("/assets/does-not-exist", headers=H1).status_code == 404


def test_list_filters_and_isolation():
    _mk_asset(client, H1, category="machinery", department="farm")
    _mk_asset(client, H1, asset_code="AST-2", category="vehicles", department="logistics")
    _mk_asset(client, H2, asset_code="AST-X", category="vehicles")
    _mk_asset(client, {"X-User-Id": U1, "X-Book-ID": BOOK_B}, asset_code="AST-B", category="land")

    all_mine = client.get("/assets", headers=H1).json()
    assert {a["asset_code"] for a in all_mine} == {"AST-1", "AST-2"}

    cats = client.get("/assets", params={"category": "vehicles"}, headers=H1).json()
    assert [a["asset_code"] for a in cats] == ["AST-2"]
    deps = client.get("/assets", params={"department": "farm"}, headers=H1).json()
    assert [a["asset_code"] for a in deps] == ["AST-1"]
    assert [a["asset_code"] for a in client.get("/assets", params={"status": "active"}, headers=H1).json()] == [
        "AST-1",
        "AST-2",
    ]

    # other user sees only their own record
    assert {a["asset_code"] for a in client.get("/assets", headers=H2).json()} == {"AST-X"}
    # other Book of the same user sees only their Book B record
    assert {a["asset_code"] for a in client.get("/assets", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()} == {
        "AST-B"
    }


def test_depreciation_semantics():
    asset = _mk_asset(client, H1, acquisition_cost=12000.0, salvage_value=0.0, useful_life_years=10).json()

    # straight line: (12000 - 0) / (10*12) = 100 per month
    e1 = client.post(f"/assets/{asset['id']}/depreciate", params={"period": "2026-01"}, headers=H1)
    assert e1.status_code == 200, e1.text
    entry = e1.json()
    assert entry["depreciation_amount"] == 100.0
    assert entry["accumulated_depreciation"] == 100.0
    assert entry["net_book_value"] == 11900.0

    # state persisted on the asset node
    got = client.get(f"/assets/{asset['id']}", headers=H1).json()
    assert got["accumulated_depreciation"] == 100.0
    assert got["net_book_value"] == 11900.0

    e2 = client.post(f"/assets/{asset['id']}/depreciate", params={"period": "2026-02"}, headers=H1).json()
    assert e2["accumulated_depreciation"] == 200.0

    entries = client.get(f"/assets/{asset['id']}/depreciation", headers=H1).json()
    assert len(entries) == 2
    # other user cannot read the entries (asset not visible)
    assert client.get(f"/assets/{asset['id']}/depreciation", headers=H2).json() == []

    # cross-user depreciation: 404
    assert client.post(f"/assets/{asset['id']}/depreciate", params={"period": "2026-03"}, headers=H2).status_code == 404

    # reducing balance: 20%/yr on NBV
    rb = _mk_asset(
        client,
        H1,
        asset_code="AST-RB",
        acquisition_cost=1200.0,
        depreciation_method="reducing_balance",
        useful_life_years=5,
    ).json()
    r = client.post(f"/assets/{rb['id']}/depreciate", params={"period": "2026-01"}, headers=H1).json()
    assert r["depreciation_amount"] == 1200.0 * 0.2 / 12

    # floor at salvage value flips status to disposed (reducing balance drops NBV
    # below salvage in one month: 100 - 1.667 = 98.33 <= 99)
    short = _mk_asset(
        client,
        H1,
        asset_code="AST-S",
        acquisition_cost=100.0,
        salvage_value=99.0,
        useful_life_years=1,
        depreciation_method="reducing_balance",
    ).json()
    assert (
        client.post(f"/assets/{short['id']}/depreciate", params={"period": "2026-01"}, headers=H1).json()[
            "net_book_value"
        ]
        == 99.0
    )
    got = client.get(f"/assets/{short['id']}", headers=H1).json()
    assert got["status"] == "disposed"
    # disposed asset cannot depreciate further: 400
    assert client.post(f"/assets/{short['id']}/depreciate", params={"period": "2026-02"}, headers=H1).status_code == 400


def test_disposal_semantics():
    asset = _mk_asset(client, H1, acquisition_cost=5000.0, useful_life_years=10).json()
    # depreciate once: NBV 4958.33...
    client.post(f"/assets/{asset['id']}/depreciate", params={"period": "2026-01"}, headers=H1)
    nbv = client.get(f"/assets/{asset['id']}", headers=H1).json()["net_book_value"]

    resp = client.post(
        f"/assets/{asset['id']}/dispose",
        params={"disposal_date": "2026-06-01", "disposal_value": nbv + 100.0},
        headers=H1,
    )
    assert resp.status_code == 200, resp.text
    assert resp.json()["gain_loss"] == 100.0

    got = client.get(f"/assets/{asset['id']}", headers=H1).json()
    assert got["status"] == "disposed"
    # double disposal: 400
    assert (
        client.post(f"/assets/{asset['id']}/dispose", params={"disposal_date": "2026-06-02"}, headers=H1).status_code
        == 400
    )
    # cross-user disposal: 404
    assert (
        client.post(f"/assets/{asset['id']}/dispose", params={"disposal_date": "2026-06-02"}, headers=H2).status_code
        == 404
    )


def test_summary_scoped_to_caller():
    _mk_asset(client, H1, acquisition_cost=12000.0, useful_life_years=10)
    _mk_asset(client, H1, asset_code="AST-2", category="vehicles", acquisition_cost=6000.0, useful_life_years=5)
    _mk_asset(client, H2, asset_code="AST-FOREIGN", acquisition_cost=999999.0, useful_life_years=1)

    s = client.get("/summary", headers=H1).json()
    assert s["total_assets"] == 2
    assert s["active_assets"] == 2
    assert s["total_acquisition_cost"] == 18000.0
    assert s["total_net_book_value"] == 18000.0
    assert set(s["by_category"]) == {"machinery", "vehicles"}
    assert s["by_category"]["vehicles"]["nbv"] == 6000.0

    s2 = client.get("/summary", headers=H2).json()
    assert s2["total_assets"] == 1
    assert s2["total_acquisition_cost"] == 999999.0
