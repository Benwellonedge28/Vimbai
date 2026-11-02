"""Book-scoping, persistence and arithmetic tests for margin-safety-service.

Covers the stated break-even calculation, the derived-from-data path, the target
margin path (including the infinity case), risk bands, listing filters, and
cross-user / cross-Book isolation now that calculations live in Neo4j.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from fastapi.testclient import TestClient
from margin_safety_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("mos_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "mos-user-1", "mos-user-2"
BOOK_A, BOOK_B = "mos-book-a", "mos-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

BASE = {
    "entity_id": "ENT-1",
    "entity_name": "Machining line",
    "period": "2026-11",
    "current_output": 2000.0,
    "current_revenue": 100000.0,
    "break_even_output": 1500.0,
    "break_even_revenue": 75000.0,
}


def _calculate(headers=H1, **overrides):
    params = dict(BASE)
    params.update(overrides)
    return client.post("/calculate", params=params, headers=headers).json()


def test_calculate_reports_units_revenue_ratio_and_band():
    body = _calculate()
    assert body["margin_of_safety_units"] == 500.0
    assert body["margin_of_safety_revenue"] == 25000.0
    assert body["margin_of_safety_percentage"] == 25.0
    assert body["margin_of_safety_ratio"] == 0.25
    assert body["risk_level"] == "low"


@pytest.mark.parametrize(
    "revenue, expected",
    [(100000.0, "low"), (90000.0, "medium"), (76000.0, "high")],
)
def test_risk_bands_are_inclusive_at_the_thresholds(revenue, expected):
    """Exactly 20% is low and exactly 10% is medium, as the original bands were."""
    body = _calculate(current_revenue=revenue)
    assert body["risk_level"] == expected


def test_zero_denominators_leave_the_ratios_at_zero():
    body = _calculate(current_output=0, current_revenue=0)
    assert body["margin_of_safety_percentage"] == 0
    assert body["margin_of_safety_ratio"] == 0
    assert body["risk_level"] == "high"


def test_calculate_from_data_derives_break_even():
    body = client.post(
        "/calculate-from-data",
        params={
            "entity_name": "Assembly",
            "selling_price": 50.0,
            "fixed_costs": 30000.0,
            "variable_cost_per_unit": 20.0,
            "expected_sales_units": 2000.0,
        },
        headers=H1,
    ).json()

    assert body["entity_name"] == "Assembly"
    assert body["break_even_units"] == 1000.0
    assert body["break_even_revenue"] == 50000.0
    assert body["margin_of_safety_units"] == 1000.0
    assert body["margin_of_safety_percentage"] == 50.0
    assert body["risk_level"] == "low"


def test_non_positive_contribution_gives_zero_break_even():
    body = client.post(
        "/calculate-from-data",
        params={
            "entity_name": "Loss line",
            "selling_price": 10.0,
            "fixed_costs": 5000.0,
            "variable_cost_per_unit": 10.0,
            "expected_sales_units": 100.0,
        },
        headers=H1,
    ).json()
    assert body["break_even_units"] == 0
    assert body["margin_of_safety_units"] == 100.0


def test_target_mos_required_sales():
    body = client.post(
        "/target-mos",
        params={"entity_name": "Machining", "break_even_revenue": 75000.0, "target_mos_percentage": 25.0},
        headers=H1,
    ).json()
    assert body["required_sales_revenue"] == 100000.0


def test_target_of_one_hundred_percent_reports_no_finite_answer():
    """The original returned infinity here, which failed to serialise and answered 500."""
    response = client.post(
        "/target-mos",
        params={"entity_name": "Machining", "break_even_revenue": 75000.0, "target_mos_percentage": 100.0},
        headers=H1,
    )
    assert response.status_code == 200
    assert response.json()["required_sales_revenue"] is None


def test_calculations_persist_and_filter():
    _calculate()
    _calculate(entity_id="ENT-2", current_revenue=90000.0)

    all_rows = client.get("/margins", headers=H1).json()["margins"]
    assert len(all_rows) == 2

    by_entity = client.get("/margins", params={"entity_id": "ENT-2"}, headers=H1).json()["margins"]
    assert [m["entity_id"] for m in by_entity] == ["ENT-2"]

    by_risk = client.get("/margins", params={"risk_level": "medium"}, headers=H1).json()["margins"]
    assert [m["entity_id"] for m in by_risk] == ["ENT-2"]


def test_calculations_are_caller_and_book_scoped():
    _calculate()

    assert client.get("/margins", headers=H2).json()["margins"] == []
    assert client.get("/margins", headers=HB).json()["margins"] == []


def test_a_second_tenant_calculation_does_not_leak_into_mine():
    _calculate()
    _calculate(headers=H2, entity_id="ENT-THEIRS")

    mine = client.get("/margins", headers=H1).json()["margins"]
    assert [m["entity_id"] for m in mine] == ["ENT-1"]


def test_caller_header_is_required_for_the_listing():
    assert client.get("/margins").status_code == 422


def test_health_reports_the_engine():
    body = client.get("/health").json()
    assert body["service"] == "margin-safety-service"
    assert body["calculation_engine"] in ("rust", "python")
