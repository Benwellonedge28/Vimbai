"""Book-scoping, persistence and CVP arithmetic tests for cvp-analysis-service.

Covers contribution, break-even, target profit, margin of safety, operating
leverage, the stateless endpoints (quick analysis, contribution, target profit,
multi-product), listing and lookup, and cross-user / cross-Book isolation now
that analyses live in Neo4j.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from cvp_analysis_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("cvp_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "cvp-user-1", "cvp-user-2"
BOOK_A, BOOK_B = "cvp-book-a", "cvp-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

BASE = {
    "entity_id": "ENT-1",
    "entity_name": "Acme",
    "selling_price_per_unit": 100.0,
    "variable_cost_per_unit": 60.0,
    "fixed_costs": 20000.0,
    "expected_sales_units": 800.0,
    "target_profit": 10000.0,
}


def _analyze(headers=H1, **overrides):
    params = dict(BASE)
    params.update(overrides)
    return client.post("/analyze", params=params, headers=headers).json()


def test_full_analysis_figures():
    body = _analyze()

    assert body["contribution_per_unit"] == 40.0
    assert body["contribution_margin_ratio"] == 0.4
    assert body["break_even_units"] == 500.0
    assert body["break_even_revenue"] == 50000.0
    assert body["units_for_target_profit"] == 750.0
    assert body["revenue_for_target_profit"] == 75000.0
    assert body["expected_sales_revenue"] == 80000.0
    assert body["margin_of_safety_units"] == 300.0
    assert body["margin_of_safety_revenue"] == 30000.0
    assert body["margin_of_safety_percentage"] == 37.5
    assert body["profit_at_expected_sales"] == 12000.0
    assert body["degree_of_operating_leverage"] == 32000.0 / 12000.0


def test_negative_contribution_leaves_break_even_empty():
    body = _analyze(selling_price_per_unit=50.0, variable_cost_per_unit=60.0)

    assert body["contribution_per_unit"] == -10.0
    assert body["contribution_margin_ratio"] == -0.2
    assert body["break_even_units"] == 0.0
    assert body["units_for_target_profit"] == 0.0
    assert body["profit_at_expected_sales"] == -28000.0


def test_zero_expected_sales_skips_margin_of_safety():
    body = _analyze(expected_sales_units=0.0)
    assert body["margin_of_safety_units"] == 0.0
    assert body["margin_of_safety_percentage"] == 0.0
    assert body["expected_sales_revenue"] == 0.0


def test_quick_analysis_is_stateless():
    body = client.post(
        "/quick-analysis", params={"fixed_costs": 20000.0, "selling_price": 100.0, "variable_cost": 60.0}, headers=H1
    ).json()
    assert body["break_even_units"] == 500.0
    assert client.get("/analyses", headers=H1).json()["analyses"] == []


def test_contribution_endpoint_is_stateless():
    body = client.post(
        "/contribution",
        json={
            "entity_id": "ENT-1",
            "entity_name": "Acme",
            "selling_price_per_unit": 100.0,
            "variable_cost_per_unit": 60.0,
            "units_sold": 500.0,
        },
        headers=H1,
    ).json()
    assert body["contribution_per_unit"] == 40.0
    assert client.get("/analyses", headers=H1).json()["analyses"] == []


def test_multi_product_break_even():
    body = client.post(
        "/multi-product",
        json={
            "entity_id": "ENT-1",
            "entity_name": "Acme",
            "fixed_costs": 30000.0,
            "products": [
                # expected_proportion is a PERCENTAGE of the sales mix (60 = 60%)
                {"product_name": "A", "selling_price": 100.0, "variable_cost": 60.0, "expected_proportion": 60},
                {"product_name": "B", "selling_price": 50.0, "variable_cost": 20.0, "expected_proportion": 40},
            ],
        },
        headers=H1,
    )
    assert body.status_code == 200, body.text
    served = body.json()
    # weighted ratio = 0.6*0.4 + 0.4*0.6 = 0.48, so break-even revenue = 30000/0.48
    assert served["weighted_contribution_margin_ratio"] == 0.48
    assert served["break_even_revenue"] == 62500.0
    assert [p["break_even_revenue_share"] for p in served["product_breakdown"]] == [37500.0, 25000.0]


def test_analyses_persist_and_are_retrievable():
    created = _analyze()

    listed = client.get("/analyses", headers=H1).json()
    assert listed["total"] == 1
    assert listed["analyses"][0]["id"] == created["id"]

    fetched = client.get(f"/analyses/{created['id']}", headers=H1).json()
    assert fetched["break_even_units"] == 500.0
    assert fetched["degree_of_operating_leverage"] == pytest.approx(32000.0 / 12000.0)


def test_listing_filters_by_entity():
    _analyze()
    _analyze(entity_id="ENT-2")

    assert client.get("/analyses", params={"entity_id": "ENT-2"}, headers=H1).json()["total"] == 1


def test_unknown_analysis_returns_404():
    assert client.get("/analyses/missing", headers=H1).status_code == 404


def test_analyses_are_caller_and_book_scoped():
    created = _analyze()

    assert client.get("/analyses", headers=H2).json()["analyses"] == []
    assert client.get("/analyses", headers=HB).json()["analyses"] == []
    assert client.get(f"/analyses/{created['id']}", headers=H2).status_code == 404


def test_caller_header_is_required():
    assert client.get("/analyses").status_code == 422


def test_health_reports_the_service():
    body = client.get("/health").json()
    assert body["service"] == "cvp-analysis-service"
