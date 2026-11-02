"""Book-scoping, persistence and behaviour tests for total-production-cost-service.

Covers the cost maths (prime, factory overhead, WIP adjustment, per-unit),
caller-scoped listings and filters, latest-per-product, the department summary,
and cross-user / cross-Book isolation now that calculations live in Neo4j.
"""

import importlib.util
import os

import main  # noqa: F401  (isort: keep bare main import first)
import pytest
from fastapi.testclient import TestClient
from total_production_cost_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("tpc_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "tpc-user-1", "tpc-user-2"
BOOK_A, BOOK_B = "tpc-book-a", "tpc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _calculate(headers=H1, **overrides):
    params = {
        "product_id": "PROD-1",
        "department_id": "DEPT-1",
        "period": "2026-10",
        "direct_materials": 1000.0,
        "direct_labor": 500.0,
        "direct_expenses": 100.0,
        "factory_rent": 300.0,
        "factory_depreciation": 150.0,
        "factory_insurance": 50.0,
        "factory_maintenance": 100.0,
        "other_overhead": 25.0,
        "wip_opening": 200.0,
        "wip_closing": 120.0,
        "units_produced": 100,
    }
    params.update(overrides)
    return client.post("/calculate", params=params, headers=headers)


def test_cost_maths():
    body = _calculate().json()

    assert body["prime_cost"] == 1600.0
    assert body["total_overhead"] == 625.0
    assert body["total_production_cost"] == 1600.0 + 625.0 + 200.0 - 120.0
    assert body["cost_per_unit"] == pytest.approx(2305.0 / 100)


def test_no_units_leaves_cost_per_unit_at_zero():
    body = _calculate(units_produced=0).json()
    assert body["total_production_cost"] == 2305.0
    assert body["cost_per_unit"] == 0


def test_calculation_is_persisted_for_its_owner():
    created = _calculate().json()

    listed = client.get("/calculations", headers=H1).json()["calculations"]
    assert [c["id"] for c in listed] == [created["id"]]


def test_listing_filters_by_product_department_and_period():
    _calculate(product_id="PROD-1")
    _calculate(product_id="PROD-2", department_id="DEPT-2", period="2026-11")

    assert len(client.get("/calculations", headers=H1).json()["calculations"]) == 2
    assert len(client.get("/calculations", params={"product_id": "PROD-2"}, headers=H1).json()["calculations"]) == 1
    assert len(client.get("/calculations", params={"department_id": "DEPT-1"}, headers=H1).json()["calculations"]) == 1
    assert len(client.get("/calculations", params={"period": "2026-11"}, headers=H1).json()["calculations"]) == 1


def test_latest_per_product():
    first = _calculate(units_produced=10).json()
    second = _calculate(units_produced=20).json()

    latest = client.get("/calculations/PROD-1/latest", headers=H1).json()
    assert latest["id"] in (first["id"], second["id"])
    assert client.get("/calculations/UNKNOWN/latest", headers=H1).json() == {"error": "Calculation not found"}


def test_summary_totals_and_average():
    _calculate(product_id="PROD-1", units_produced=100)
    _calculate(product_id="PROD-2", units_produced=50)

    summary = client.get("/summary", headers=H1).json()
    assert summary["calculations_count"] == 2
    assert summary["total_prime_cost"] == 3200.0
    assert summary["total_overhead"] == 1250.0
    assert summary["total_production_cost"] == 4610.0
    assert summary["total_units_produced"] == 150
    assert summary["average_cost_per_unit"] == pytest.approx(4610.0 / 150)


def test_summary_with_no_calculations_is_zeroed():
    summary = client.get("/summary", headers=H1).json()
    assert summary == {
        "total_prime_cost": 0,
        "total_overhead": 0,
        "total_production_cost": 0,
        "total_units_produced": 0,
        "average_cost_per_unit": 0,
        "calculations_count": 0,
    }


def test_another_user_cannot_see_the_calculations():
    _calculate(headers=H1)

    assert client.get("/calculations", headers=H2).json()["calculations"] == []
    assert client.get("/calculations/PROD-1/latest", headers=H2).json() == {"error": "Calculation not found"}
    assert client.get("/summary", headers=H2).json()["calculations_count"] == 0


def test_another_book_cannot_see_the_calculations():
    _calculate(headers=H1)

    assert client.get("/calculations", headers=HB).json()["calculations"] == []
    assert client.get("/summary", headers=HB).json()["calculations_count"] == 0


def test_caller_header_is_required():
    assert client.get("/calculations").status_code == 422
