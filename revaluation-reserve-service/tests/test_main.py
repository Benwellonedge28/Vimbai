"""Book-scoping and persistence tests for revaluation-reserve-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from revaluation_reserve_service.database import Neo4jConnector

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
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _record_reval(client, headers, **kw):
    params = {
        "company_id": "co-1",
        "asset_id": "asset-b1",
        "asset_name": "Office Block",
        "asset_class": "property",
        "revaluation_date": "2026-04-01",
        "previous_value": 100000.0,
        "new_value": 130000.0,
    }
    params.update(kw)
    return client.post("/revaluations/record", params=params, headers=headers)


def test_health():
    assert client.get("/health").json()["status"] == "healthy"


def test_record_revaluation_and_isolation():
    r = _record_reval(client, H1)
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["revaluation"]["revaluation_gain"] == 30000.0
    assert body["revaluation"]["net_effect"] == 30000.0
    assert body["cumulative"]["net_revaluation_reserve"] == 30000.0

    # U2 records against the same asset id — separate cumulative, no leak
    _record_reval(client, H2, company_id="co-2", new_value=200000.0)

    listed = client.get("/revaluations", headers=H1).json()["revaluations"]
    assert len(listed) == 1
    assert client.get("/revaluations", headers=H2).json()["revaluations"][0]["revaluation_gain"] == 100000.0

    # cumulative is per caller+asset
    c1 = client.get("/cumulative/asset-b1", headers=H1).json()
    c2 = client.get("/cumulative/asset-b1", headers=H2).json()
    assert c1["net_revaluation_reserve"] == 30000.0
    assert c2["net_revaluation_reserve"] == 100000.0

    # Book-gated: same user, other Book sees nothing
    assert client.get("/cumulative/asset-b1", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json() == {
        "error": "Asset not found"
    }
    assert client.get("/revaluations", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()["revaluations"] == []


def test_cumulative_upsert_accumulates():
    _record_reval(client, H1, previous_value=100000.0, new_value=130000.0)
    _record_reval(client, H1, previous_value=130000.0, new_value=140000.0)  # gain of 10k
    _record_reval(client, H1, previous_value=140000.0, new_value=135000.0)  # loss of 5k

    c = client.get("/cumulative/asset-b1", headers=H1).json()
    assert c["total_revaluation_gain"] == 40000.0
    assert c["total_revaluation_loss"] == 5000.0
    assert c["net_revaluation_reserve"] == 35000.0

    # downward revaluation gain=0, loss=previous-new
    down = _record_reval(client, H1, previous_value=135000.0, new_value=120000.0).json()
    assert down["revaluation"]["revaluation_loss"] == 15000.0
    assert down["revaluation"]["revaluation_gain"] == 0.0
    assert down["cumulative"]["total_revaluation_loss"] == 20000.0


def test_utilization_and_summary_scoped():
    _record_reval(client, H1, asset_id="asset-c7", asset_name="Fleet", asset_class="equipment")

    u = client.post(
        "/utilizations/record",
        params={
            "company_id": "co-1",
            "amount": 4000.0,
            "utilization_type": "impairment",
            "related_asset_id": "asset-c7",
            "description": "Impairment charge",
        },
        headers=H1,
    )
    assert u.status_code == 200, u.text
    assert u.json()["utilization_type"] == "impairment"

    # foreign utilization must not leak into U1's summary
    client.post(
        "/utilizations/record",
        params={"company_id": "co-1", "amount": 50000.0, "utilization_type": "asset_disposal", "description": "x"},
        headers=H2,
    )

    s = client.get("/summary/co-1", headers=H1).json()
    assert s["total_revaluation_gains"] == 30000.0
    assert s["total_revaluation_losses"] == 0.0
    assert s["total_utilized"] == 4000.0
    assert s["net_revaluation_reserve"] == 26000.0
    assert s["assets_revalued"] == 1

    s2 = client.get("/summary/co-1", headers=H2).json()
    assert s2["total_utilized"] == 50000.0
