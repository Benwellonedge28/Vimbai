"""Book-scoping, persistence and decision tests for order-acceptance-service.

Covers the accept/reject logic for a special order (relevant costing, displaced
sales, incremental fixed costs), the quick one-time-order check, listing
filters, and cross-user / cross-Book isolation now that decisions live in Neo4j.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from fastapi.testclient import TestClient
from order_acceptance_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("oa_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "oa-user-1", "oa-user-2"
BOOK_A, BOOK_B = "oa-book-a", "oa-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

BASE = {
    "order_id": "ORD-1",
    "product_id": "P-1",
    "product_name": "Widget",
    "customer_name": "Acme",
    "normal_selling_price": 100.0,
    "offered_price": 70.0,
    "variable_cost_per_unit": 40.0,
    "units_requested": 500.0,
}


def _analyze(headers=H1, **overrides):
    params = dict(BASE)
    params.update(overrides)
    return client.post("/analyze", params=params, headers=headers).json()


def test_profitable_special_order_is_accepted():
    body = _analyze()
    assert body["price_below_normal"] == 30.0
    assert body["price_below_normal_percentage"] == 30.0
    assert body["contribution_per_unit_at_normal"] == 60.0
    assert body["contribution_per_unit_at_offered"] == 30.0
    assert body["contribution_from_order"] == 15000.0
    assert body["minimum_acceptable_price"] == 40.0
    assert body["net_contribution"] == 15000.0
    assert body["recommendation"] == "ACCEPT"
    assert body["conditions"] == ["Positive contribution generated"]


def test_order_below_variable_cost_is_rejected():
    body = _analyze(offered_price=35.0)
    assert body["contribution_per_unit_at_offered"] == -5.0
    assert body["contribution_from_order"] == -2500.0
    assert body["recommendation"] == "REJECT"


def test_displaced_regular_sales_are_charged_as_opportunity_cost():
    body = _analyze(displaced_sales_units=200, displaced_contribution_per_unit=60.0)
    assert body["opportunity_cost"] == 12000.0
    assert body["net_contribution"] == 3000.0
    # displacing regular sales always attaches conditions, even when still profitable
    assert body["recommendation"] == "ACCEPT_WITH_CONDITIONS"


def test_incremental_fixed_costs_are_reported_but_not_charged_to_net_contribution():
    """Original quirk: only variable cost and displaced contribution move the decision."""
    body = _analyze(incremental_fixed_costs=20000.0)
    assert body["total_fixed_cost_incremental"] == 20000.0
    assert body["net_contribution"] == 15000.0
    assert body["recommendation"] == "ACCEPT"


def test_displaced_sales_can_make_the_order_unattractive():
    body = _analyze(displaced_sales_units=500, displaced_contribution_per_unit=60.0)
    assert body["net_contribution"] == -15000.0
    assert body["recommendation"] == "REJECT"


def test_full_cost_is_carried_when_supplied():
    body = _analyze(full_cost_per_unit=55.0)
    assert body["full_cost_per_unit"] == 55.0


def test_one_time_order_with_spare_capacity_accepts_any_positive_contribution():
    body = client.post(
        "/one-time-order",
        params={
            "product_name": "Widget",
            "offered_price": 45.0,
            "variable_cost_per_unit": 40.0,
            "units": 100.0,
            "has_spare_capacity": True,
        },
        headers=H1,
    ).json()
    assert body["recommendation"] == "ACCEPT"
    assert client.get("/decisions", headers=H1).json()["decisions"] == []


def test_decisions_persist_and_filter():
    _analyze()
    _analyze(order_id="ORD-2", product_id="P-2", offered_price=35.0)

    assert len(client.get("/decisions", headers=H1).json()["decisions"]) == 2
    by_order = client.get("/decisions", params={"order_id": "ORD-2"}, headers=H1).json()["decisions"]
    assert [d["order_id"] for d in by_order] == ["ORD-2"]
    rejected = client.get("/decisions", params={"recommendation": "REJECT"}, headers=H1).json()["decisions"]
    assert [d["order_id"] for d in rejected] == ["ORD-2"]


def test_decisions_are_caller_and_book_scoped():
    _analyze()

    assert client.get("/decisions", headers=H2).json()["decisions"] == []
    assert client.get("/decisions", headers=HB).json()["decisions"] == []


def test_caller_header_is_required_for_the_listing():
    assert client.get("/decisions").status_code == 422


def test_health_reports_the_engine():
    body = client.get("/health").json()
    assert body["service"] == "order-acceptance-service"
    assert body["calculation_engine"] in ("rust", "python")
