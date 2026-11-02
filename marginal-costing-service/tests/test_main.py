"""Book-scoping, persistence and arithmetic tests for marginal-costing-service.

Covers cost items, the marginal income statement, contribution analysis, the
profit forecast that persists nothing, the filtered listings and the summary,
plus cross-user / cross-Book isolation now that the three stores live in Neo4j.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from fastapi.testclient import TestClient
from marginal_costing_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("mc_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "mc-user-1", "mc-user-2"
BOOK_A, BOOK_B = "mc-book-a", "mc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

STATEMENT = {
    "company_id": "comp-1",
    "period": "2026-11",
    "sales_revenue": 900000.0,
    "fixed_costs": 200000.0,
}
VARIABLE_COSTS = {"materials": 300000.0, "labour": 150000.0}


def _add_cost(name="Steel", code="ST", variable=600.0, fixed=0.0, units=4.0, driver="units", headers=H1):
    return client.post(
        "/costs/add",
        params={
            "cost_name": name,
            "cost_code": code,
            "variable_cost": variable,
            "fixed_cost": fixed,
            "units": units,
            "cost_driver": driver,
        },
        headers=headers,
    ).json()


def _statement(headers=H1, **overrides):
    params = dict(STATEMENT)
    params.update(overrides)
    return client.post("/income-statement/generate", params=params, json=VARIABLE_COSTS, headers=headers).json()


def test_cost_item_reports_total_and_unit_variable_cost():
    body = _add_cost()
    assert body["total_cost"] == 600.0
    assert body["unit_variable_cost"] == 150.0
    assert body["cost_driver"] == "units"


def test_income_statement_splits_contribution_and_profit():
    body = _statement()
    assert body["total_variable_costs"] == 450000.0
    assert body["contribution"] == 450000.0
    assert body["profit"] == 250000.0
    assert body["variable_cost_breakdown"] == VARIABLE_COSTS


def test_contribution_analysis_per_unit_ratio_and_profit():
    body = client.post(
        "/contribution/analyze",
        params={
            "product_id": "P-1",
            "selling_price": 100.0,
            "variable_cost_per_unit": 60.0,
            "units_sold": 500.0,
            "fixed_costs": 5000.0,
        },
        headers=H1,
    ).json()

    assert body["contribution_per_unit"] == 40.0
    assert body["contribution_margin_ratio"] == 40.0
    assert body["total_contribution"] == 20000.0
    assert body["profit_from_product"] == 15000.0


def test_profit_forecast_is_computed_without_persisting():
    body = client.post(
        "/profit-forecast",
        params={
            "selling_price": 100.0,
            "variable_cost_per_unit": 60.0,
            "fixed_costs": 5000.0,
            "expected_units": 500.0,
        },
        headers=H1,
    ).json()

    assert body["total_contribution"] == 20000.0
    assert body["forecast_profit"] == 15000.0
    assert client.get("/contributions", headers=H1).json()["contributions"] == []


def test_summary_aggregates_the_callers_cost_items():
    _add_cost(variable=600.0, fixed=100.0)
    _add_cost(name="Labour", code="LB", variable=400.0, fixed=50.0)

    body = client.get("/summary", headers=H1).json()
    assert body == {
        "total_variable_costs": 1000.0,
        "total_fixed_costs": 150.0,
        "total_costs": 1150.0,
        "cost_items": 2,
    }


def test_listings_persist_and_filter():
    _statement()
    _statement(period="2026-12")
    client.post(
        "/contribution/analyze",
        params={"product_id": "P-1", "selling_price": 100.0, "variable_cost_per_unit": 60.0, "units_sold": 500.0},
        headers=H1,
    )

    assert len(client.get("/income-statements", headers=H1).json()["statements"]) == 2
    by_period = client.get("/income-statements", params={"period": "2026-12"}, headers=H1).json()["statements"]
    assert [s["period"] for s in by_period] == ["2026-12"]
    assert len(client.get("/contributions", params={"product_id": "P-1"}, headers=H1).json()["contributions"]) == 1
    assert client.get("/contributions", params={"product_id": "P-2"}, headers=H1).json()["contributions"] == []


def test_records_are_caller_and_book_scoped():
    _add_cost()
    _statement()

    assert client.get("/summary", headers=H2).json()["cost_items"] == 0
    assert client.get("/income-statements", headers=H2).json()["statements"] == []
    assert client.get("/summary", headers=HB).json()["cost_items"] == 0


def test_caller_header_is_required_for_reads():
    assert client.get("/summary").status_code == 422
    assert client.get("/income-statements").status_code == 422


def test_health_reports_the_engine():
    body = client.get("/health").json()
    assert body["service"] == "marginal-costing-service"
    assert body["calculation_engine"] in ("rust", "python")
