"""Book-scoping, persistence and behaviour tests for lifecycle-costing-service.

Covers the phase totals (one-time plus annual across the duration), the annual
equivalent cost, phase persistence as JSON, caller-scoped listing and lookup,
model comparison, and cross-user / cross-Book isolation now that models live in
Neo4j.
"""

import importlib.util
import os

import main  # noqa: F401  (isort: keep bare main import first)
import pytest
from fastapi.testclient import TestClient
from lifecycle_costing_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("lc_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "lc-user-1", "lc-user-2"
BOOK_A, BOOK_B = "lc-book-a", "lc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

PHASES = [
    {"phase": "acquisition", "duration_years": 0, "one_time_cost": 120000.0},
    {"phase": "operation", "duration_years": 5, "annual_cost": 8000.0},
    {"phase": "disposal", "duration_years": 0, "one_time_cost": 5000.0},
]


def _create(headers=H1, phases=None, **overrides):
    # phases is the request body (a bare list); the scalar fields ride in the query.
    params = {"asset_name": "Truck", "description": "Delivery vehicle"}
    params.update(overrides)
    return client.post("/models", params=params, json=phases or PHASES, headers=headers)


def test_phase_totals_and_annual_equivalent():
    body = _create().json()

    assert body["total_lifecycle_cost"] == 165000.0
    assert body["annual_equivalent_cost"] == 33000.0
    assert [p["phase"] for p in body["phases"]] == ["acquisition", "operation", "disposal"]
    assert body["discount_rate"] == 0.1


def test_model_without_duration_has_no_annual_equivalent():
    body = _create(phases=[{"phase": "acquisition", "one_time_cost": 900.0}]).json()
    assert body["total_lifecycle_cost"] == 900.0
    assert body["annual_equivalent_cost"] == 0


def test_phases_survive_a_round_trip():
    created = _create().json()
    fetched = client.get(f"/models/{created['id']}", headers=H1).json()
    assert fetched["phases"] == created["phases"]
    assert fetched["total_lifecycle_cost"] == 165000.0


def test_listing_is_caller_scoped():
    created = _create().json()
    listed = client.get("/models", headers=H1).json()
    assert [m["id"] for m in listed] == [created["id"]]


def test_unknown_model_is_404():
    assert client.get("/models/nope", headers=H1).status_code == 404


def test_compare_reports_the_lowest_cost_model():
    cheap = _create(asset_name="Bike", phases=[{"phase": "acquisition", "one_time_cost": 1000.0}]).json()
    dear = _create(asset_name="Truck").json()

    body = client.post("/compare", json=[cheap["id"], dear["id"]], headers=H1).json()
    assert len(body["comparison"]) == 2
    assert body["lowest_cost_model"] == cheap["id"]


def test_compare_without_a_match_is_404():
    assert client.post("/compare", json=["missing"], headers=H1).status_code == 404


def test_another_user_sees_nothing_and_cannot_compare():
    created = _create().json()

    assert client.get("/models", headers=H2).json() == []
    assert client.get(f"/models/{created['id']}", headers=H2).status_code == 404
    assert client.post("/compare", json=[created["id"]], headers=H2).status_code == 404


def test_another_book_sees_nothing():
    created = _create().json()

    assert client.get("/models", headers=HB).json() == []
    assert client.get(f"/models/{created['id']}", headers=HB).status_code == 404


def test_caller_header_is_required():
    assert client.get("/models").status_code == 422


def test_health_still_reports_the_service():
    body = client.get("/health").json()
    assert body["service"] == "lifecycle-costing-service"
    assert body["status"] == "healthy"
