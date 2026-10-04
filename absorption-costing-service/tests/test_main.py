"""Book-scoping, persistence and behaviour tests for absorption-costing-service (fake Neo4j harness).

Covers: product-cost calculation math (prime, total, per-unit), cost
components, overhead absorption rate math, cost-plus purity, caller-
scoped listings and filters, latest-per-product, stock valuation, and
cross-user / cross-Book isolation. Journal-entry posting to the
accounting service is a side effect (httpx call) and is not asserted.
"""

import importlib.util
import os

import main  # noqa: F401  (isort: keep bare main import first)
import pytest
from absorption_costing_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("abc_cost_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "abs-user-1", "abs-user-2"
BOOK_A, BOOK_B = "abs-book-a", "abs-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _calc(product_id="PROD-1", period="2026-09", headers=H1, components=None, **extra):
    params = {
        "product_id": product_id,
        "product_name": "Widget",
        "period": period,
        "direct_materials": 1000.0,
        "direct_labor": 500.0,
        "direct_expenses": 200.0,
        "manufacturing_overhead": 300.0,
        "units_produced": 100,
        "opening_stock": 10,
        "closing_stock": 20,
    }
    params.update(extra)
    r = client.post(
        "/product-costs/calculate",
        params=params,
        json=components if components else None,
        headers=headers,
    )
    assert r.status_code == 200, r.text
    return r.json()


def test_product_cost_calculation_and_persistence():
    pc = _calc(components=[{"component_name": "glue", "amount": 50.0, "cost_type": "direct_expense"}])

    assert pc["prime_cost"] == 1700.0
    assert pc["total_production_cost"] == 2000.0
    assert pc["cost_per_unit"] == 20.0
    assert len(pc["cost_components"]) == 1
    assert pc["cost_components"][0]["component_name"] == "glue"

    # persisted: shows up in the caller's listing
    listed = client.get("/product-costs", headers=H1).json()["product_costs"]
    assert len(listed) == 1
    assert listed[0]["id"] == pc["id"]
    assert listed[0]["prime_cost"] == 1700.0

    # zero units: no divide-by-zero, per-unit stays 0
    pc0 = _calc(product_id="PROD-0", units_produced=0)
    assert pc0["cost_per_unit"] == 0.0


def test_product_cost_filters_and_latest():
    _calc(product_id="PROD-A", period="2026-09")
    _calc(product_id="PROD-A", period="2026-10")
    _calc(product_id="PROD-B", period="2026-09")

    assert len(client.get("/product-costs", params={"product_id": "PROD-A"}, headers=H1).json()["product_costs"]) == 2
    assert len(client.get("/product-costs", params={"period": "2026-09"}, headers=H1).json()["product_costs"]) == 2
    assert (
        len(
            client.get("/product-costs", params={"product_id": "PROD-A", "period": "2026-10"}, headers=H1).json()[
                "product_costs"
            ]
        )
        == 1
    )

    latest = client.get("/product-costs/PROD-A/latest", headers=H1).json()
    assert latest["period"] == "2026-10"

    assert client.get("/product-costs/NOPE/latest", headers=H1).json() == {"error": "Product cost not found"}


def test_overhead_absorption_math_and_persistence():
    params = {
        "product_id": "PROD-1",
        "period": "2026-09",
        "overhead_cost": 1200.0,
        "absorption_base": "machine_hours",
        "absorption_base_units": 600.0,
    }
    r = client.post("/overhead/absorption", params=params, headers=H1)
    assert r.status_code == 200
    a = r.json()
    assert a["overhead_absorption_rate"] == 2.0
    assert a["absorbed_overhead"] == 1200.0

    # zero base units: rates stay 0
    r = client.post(
        "/overhead/absorption",
        params={
            "product_id": "PROD-2",
            "period": "2026-09",
            "overhead_cost": 500.0,
            "absorption_base": "units",
            "absorption_base_units": 0,
        },
        headers=H1,
    )
    assert r.json()["overhead_absorption_rate"] == 0.0


def test_cost_plus_is_pure():
    r = client.post("/cost-plus", params={"product_cost": 2000.0, "markup_percentage": 25.0}, headers=H1)
    assert r.status_code == 200
    data = r.json()
    assert data["markup_amount"] == 500.0
    assert data["selling_price"] == 2500.0


def test_stock_valuation():
    _calc(product_id="PROD-S", closing_stock=20)

    r = client.get("/stock-valuation", params={"product_id": "PROD-S"}, headers=H1)
    assert r.status_code == 200
    data = r.json()
    assert data["cost_per_unit"] == 20.0
    assert data["closing_stock_units"] == 20
    assert data["closing_stock_value"] == 400.0
    assert data["valuation_method"] == "fifo"

    # unknown product keeps the original error contract
    assert client.get("/stock-valuation", params={"product_id": "NOPE"}, headers=H1).json() == {
        "error": "Product cost not found"
    }


def test_cross_user_and_book_isolation():
    _calc(product_id="PROD-X", headers=H1)
    _calc(product_id="PROD-Y", headers=H2)
    _calc(product_id="PROD-Z", headers=HB)

    assert len(client.get("/product-costs", headers=H1).json()["product_costs"]) == 1
    assert len(client.get("/product-costs", headers=H2).json()["product_costs"]) == 1
    assert len(client.get("/product-costs", headers=HB).json()["product_costs"]) == 1

    # latest and valuation only see the caller's records
    assert client.get("/product-costs/PROD-Y/latest", headers=H1).json() == {"error": "Product cost not found"}
    assert client.get("/stock-valuation", params={"product_id": "PROD-X"}, headers=H2).json() == {
        "error": "Product cost not found"
    }
