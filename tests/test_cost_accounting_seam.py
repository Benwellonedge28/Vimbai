"""Parity between the cost accounting seam and the Python services.

Absorption, marginal and target costing now cross the Rust seam. These tests run
the same request through the service and the extension and compare every figure,
because the seam only earns its place if it reproduces what the services already
report.
"""

import importlib.util
import json
import os
import sys
import types

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service

native = pytest.importorskip("vimbai_native", reason="build library/native with maturin to test the seam")

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_H = {"X-User-Id": "seam-cost-user", "X-Book-ID": "seam-cost-book"}


def _absorption_client():
    """Load the absorption service and bind its fake graph, the way its own tests do."""
    app = load_service("absorption-costing-service").main.app
    service_dir = os.path.join(REPO_ROOT, "absorption-costing-service")

    fake_path = os.path.join(service_dir, "fake_neo4j.py")
    spec = importlib.util.spec_from_file_location("seam_absorption_fake", fake_path)
    fake = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fake)

    pkg_name = "absorption_costing_service"
    pkg = sys.modules.get(pkg_name)
    if pkg is None or not hasattr(pkg, "__path__"):
        alias = types.ModuleType(pkg_name)
        alias.__path__ = [service_dir]
        sys.modules[pkg_name] = alias

    db = importlib.import_module(f"{pkg_name}.database")
    session = fake.FakeSession()
    db.Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(session))
    return TestClient(app)


def _seam(fn, payload):
    return json.loads(getattr(native, fn)(json.dumps(payload)))


@pytest.mark.parametrize(
    "units, overhead",
    [(10, 500.0), (0, 240.0), (3, 0.0)],
)
def test_absorption_product_cost_matches_the_service(units, overhead):
    client = _absorption_client()
    params = {
        "product_id": "PRD-1",
        "product_name": "Widget",
        "period": "2026-10",
        "direct_materials": 1200.0,
        "direct_labor": 800.0,
        "direct_expenses": 150.0,
        "manufacturing_overhead": overhead,
        "units_produced": units,
        "opening_stock": 5,
        "closing_stock": 2,
    }
    served = client.post("/product-costs/calculate", params=params, headers=_H)
    assert served.status_code == 200, served.text
    service = served.json()

    rust = _seam("absorption_product_cost_json", params)

    for key in ("prime_cost", "total_production_cost", "cost_per_unit"):
        assert service[key] == rust[key], key


@pytest.mark.parametrize("units", [200.0, 0.0])
def test_absorption_overhead_matches_the_service(units):
    client = _absorption_client()
    params = {
        "product_id": "PRD-2",
        "period": "2026-10",
        "overhead_cost": 9000.0,
        "absorption_base": "machine_hours",
        "absorption_base_units": units,
    }
    served = client.post("/overhead/absorption", params=params, headers=_H)
    assert served.status_code == 200, served.text
    service = served.json()

    rust = _seam("absorption_overhead_json", params)

    assert service["overhead_absorption_rate"] == rust["overhead_absorption_rate"]
    assert service["absorbed_overhead"] == rust["absorbed_overhead"]


@pytest.mark.parametrize("cost, markup", [(250.0, 12.5), (0.0, 40.0), (99.99, 0.0)])
def test_cost_plus_matches_the_service(cost, markup):
    client = _absorption_client()
    served = client.post("/cost-plus", params={"product_cost": cost, "markup_percentage": markup})
    assert served.status_code == 200, served.text
    service = served.json()

    rust = _seam("absorption_cost_plus_json", {"product_cost": cost, "markup_percentage": markup})

    for key in ("markup_amount", "selling_price"):
        assert service[key] == rust[key], key


@pytest.mark.parametrize(
    "body",
    [
        {
            "company_id": "comp-1",
            "product_name": "Widget",
            "target_selling_price": 500.0,
            "desired_profit_margin_pct": 25.0,
            "current_cost": 420.0,
            "component_costs": [["materials", 180.0], ["labour", 120.0], ["overhead", 120.0]],
        },
        {
            "company_id": "comp-2",
            "product_name": "Loss-maker",
            "target_selling_price": 100.0,
            "desired_profit_margin_pct": 10.0,
            "current_cost": 140.0,
            "component_costs": [],
        },
        {
            "company_id": "comp-3",
            "product_name": "Zero cost",
            "target_selling_price": 80.0,
            "desired_profit_margin_pct": 20.0,
            "current_cost": 0.0,
            "component_costs": [["parts", 0.0]],
        },
    ],
)
def test_target_costing_matches_the_service(body):
    client = TestClient(load_service("target-costing-service").main.app)
    payload = dict(body)
    payload["component_costs"] = dict(body["component_costs"])

    served = client.post("/calculate", json=payload)
    assert served.status_code == 200, served.text
    service = served.json()

    rust = _seam("target_cost_json", body)

    for key in (
        "target_profit",
        "target_cost",
        "cost_reduction_needed",
        "cost_reduction_pct",
        "feasible",
        "target_selling_price",
        "desired_margin",
    ):
        assert service[key] == rust[key], key

    assert [c["component"] for c in service["component_analysis"]] == [
        c["component"] for c in rust["component_analysis"]
    ]
    for served_component, rust_component in zip(service["component_analysis"], rust["component_analysis"]):
        assert served_component == rust_component


@pytest.mark.parametrize(
    "params",
    [
        {
            "selling_price": 50.0,
            "variable_cost_per_unit": 30.0,
            "expected_units": 1200.0,
            "fixed_costs": 15000.0,
        },
        {
            "selling_price": 0.0,
            "variable_cost_per_unit": 10.0,
            "expected_units": 400.0,
            "fixed_costs": 0.0,
        },
    ],
)
def test_marginal_profit_forecast_matches_the_service(params):
    client = TestClient(load_service("marginal-costing-service").main.app)
    served = client.post("/profit-forecast", params=params)
    assert served.status_code == 200, served.text
    service = served.json()

    rust = _seam("marginal_forecast_json", params)

    for key in (
        "contribution_per_unit",
        "contribution_margin_ratio",
        "total_contribution",
        "forecast_profit",
    ):
        assert service[key] == rust[key], key


@pytest.mark.parametrize(
    "params",
    [
        {
            "product_id": "P-1",
            "selling_price": 75.0,
            "variable_cost_per_unit": 45.0,
            "units_sold": 3000.0,
            "fixed_costs": 60000.0,
        },
        {
            "product_id": "P-2",
            "selling_price": 0.0,
            "variable_cost_per_unit": 5.0,
            "units_sold": 10.0,
            "fixed_costs": 0.0,
        },
    ],
)
def test_marginal_contribution_matches_the_service(params):
    client = TestClient(load_service("marginal-costing-service").main.app)
    served = client.post("/contribution/analyze", params=params)
    assert served.status_code == 200, served.text
    service = served.json()

    rust = _seam("marginal_contribution_json", params)

    for key in (
        "contribution_per_unit",
        "contribution_margin_ratio",
        "total_contribution",
        "profit_from_product",
    ):
        assert service[key] == rust[key], key


@pytest.mark.parametrize(
    "body",
    [
        {
            "company_id": "comp-1",
            "period": "2026-10",
            "sales_revenue": 900000.0,
            "variable_costs": {"materials": 300000.0, "labour": 150000.0},
            "fixed_costs": 200000.0,
        },
        {
            "company_id": "comp-2",
            "period": "2026-10",
            "sales_revenue": 0.0,
            "variable_costs": {},
            "fixed_costs": 50000.0,
        },
    ],
)
def test_marginal_income_statement_matches_the_service(body):
    client = TestClient(load_service("marginal-costing-service").main.app)
    # the scalar figures are query parameters; only the breakdown travels in the body
    served = client.post(
        "/income-statement/generate",
        params={
            "company_id": body["company_id"],
            "period": body["period"],
            "sales_revenue": body["sales_revenue"],
            "fixed_costs": body["fixed_costs"],
        },
        json=body["variable_costs"],
    )
    assert served.status_code == 200, served.text
    service = served.json()

    rust = _seam("marginal_income_statement_json", body)

    for key in ("total_variable_costs", "contribution", "profit"):
        assert service[key] == rust[key], key


class TestEngineSwitch:
    """Both engines must answer with the same figures, and /health must say which."""

    def _absorption(self, monkeypatch, engine):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        return _absorption_client()

    @pytest.mark.parametrize("engine", ["python", "rust"])
    def test_absorption_answers_are_engine_independent(self, monkeypatch, engine):
        client = self._absorption(monkeypatch, engine)
        health = client.get("/health").json()
        assert health["calculation_engine"] == engine

        params = {
            "product_id": "PRD-9",
            "product_name": "Widget",
            "period": "2026-10",
            "direct_materials": 1200.0,
            "direct_labor": 800.0,
            "direct_expenses": 150.0,
            "manufacturing_overhead": 240.0,
            "units_produced": 7,
        }
        body = client.post("/product-costs/calculate", params=params, headers=_H).json()
        assert body["prime_cost"] == 2150.0
        assert body["total_production_cost"] == 2390.0
        assert body["cost_per_unit"] == 2390.0 / 7

        absorbed = client.post(
            "/overhead/absorption",
            params={
                "product_id": "PRD-9",
                "period": "2026-10",
                "overhead_cost": 9000.0,
                "absorption_base": "machine_hours",
                "absorption_base_units": 120.0,
            },
            headers=_H,
        ).json()
        assert absorbed["overhead_absorption_rate"] == 75.0
        assert absorbed["absorbed_overhead"] == 9000.0

        priced = client.post("/cost-plus", params={"product_cost": 250.0, "markup_percentage": 12.5})
        assert priced.json()["selling_price"] == 281.25

    @pytest.mark.parametrize("engine", ["python", "rust"])
    def test_marginal_answers_are_engine_independent(self, monkeypatch, engine):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        client = TestClient(load_service("marginal-costing-service").main.app)
        assert client.get("/health").json()["calculation_engine"] == engine

        forecast = client.post(
            "/profit-forecast",
            params={
                "selling_price": 50.0,
                "variable_cost_per_unit": 30.0,
                "fixed_costs": 15000.0,
                "expected_units": 1200.0,
            },
        ).json()
        assert forecast["contribution_per_unit"] == 20.0
        assert forecast["total_contribution"] == 24000.0
        assert forecast["forecast_profit"] == 9000.0

        statement = client.post(
            "/income-statement/generate",
            params={
                "company_id": "comp-1",
                "period": "2026-10",
                "sales_revenue": 900000.0,
                "fixed_costs": 200000.0,
            },
            json={"materials": 300000.0, "labour": 150000.0},
        ).json()
        assert statement["total_variable_costs"] == 450000.0
        assert statement["contribution"] == 450000.0
        assert statement["profit"] == 250000.0
