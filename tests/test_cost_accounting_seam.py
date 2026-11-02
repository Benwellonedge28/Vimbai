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


class TestPrimeCostAndThroughput:
    """The same requests must answer identically through both engines."""

    @pytest.mark.parametrize("engine", ["python", "rust"])
    def test_prime_cost_matches_the_service(self, monkeypatch, engine):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        client = TestClient(load_service("prime-cost-service").main.app)
        assert client.get("/health").json()["calculation_engine"] == engine

        params = {
            "product_id": "PRD-1",
            "period": "2026-10",
            "direct_materials": 640.0,
            "direct_labor": 260.0,
            "direct_expenses": 40.5,
        }
        served = client.post("/calculate", params=params)
        assert served.status_code == 200, served.text
        service = served.json()

        rust = _seam("prime_cost_json", params)

        assert service["prime_cost"] == rust["prime_cost"] == 940.5
        assert service["cost_breakdown"] == rust["cost_breakdown"]

    @pytest.mark.parametrize("engine", ["python", "rust"])
    def test_prime_cost_from_items_matches_the_service(self, monkeypatch, engine):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        module = load_service("prime-cost-service").main
        module.direct_cost_items.clear()  # the service keeps its items in memory
        client = TestClient(module.app)

        items = [
            {"item_name": "Steel", "item_type": "direct_material", "amount": 300.0, "units": 10, "product_id": "PRD-7"},
            {"item_name": "Brass", "item_type": "direct_material", "amount": 120.0, "units": 4, "product_id": "PRD-7"},
            {"item_name": "Fitter", "item_type": "direct_labor", "amount": 450.0, "units": 1, "product_id": "PRD-7"},
            {"item_name": "Freight", "item_type": "direct_expense", "amount": 30.0, "units": 1, "product_id": "PRD-7"},
            {
                "item_name": "Other job",
                "item_type": "direct_material",
                "amount": 999.0,
                "units": 1,
                "product_id": "PRD-8",
            },
        ]
        for item in items:
            assert client.post("/direct-costs/add", params=item).status_code == 200

        served = client.post("/calculate-with-items", params={"product_id": "PRD-7", "period": "2026-10"})
        assert served.status_code == 200, served.text
        service = served.json()

        rust = _seam(
            "prime_cost_from_items_json",
            {"product_id": "PRD-7", "period": "2026-10", "items": items},
        )

        assert service["prime_cost"] == rust["prime_cost"] == 900.0
        assert service["cost_breakdown"] == rust["cost_breakdown"]

    @pytest.mark.parametrize("engine", ["python", "rust"])
    @pytest.mark.parametrize(
        "body",
        [
            {
                "company_id": "comp-1",
                "operating_expenses": 4000.0,
                "available_constraint_minutes": 480,
                "products": [
                    {"name": "A", "selling_price": 120.0, "material_cost": 60.0, "time_on_constraint": 4, "demand": 40},
                    {"name": "B", "selling_price": 90.0, "material_cost": 30.0, "time_on_constraint": 10, "demand": 30},
                    {"name": "C", "selling_price": 200.0, "material_cost": 120.0, "time_on_constraint": 0, "demand": 5},
                ],
            },
            {
                "company_id": "comp-2",
                "operating_expenses": 0.0,
                "available_constraint_minutes": 240,
                "products": [
                    {
                        "name": "Only",
                        "selling_price": 50.0,
                        "material_cost": 50.0,
                        "time_on_constraint": 3,
                        "demand": 100,
                    },
                ],
            },
        ],
    )
    def test_throughput_accounting_matches_the_service(self, monkeypatch, engine, body):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        client = TestClient(load_service("throughput-accounting-service").main.app)
        assert client.get("/health").json()["calculation_engine"] == engine

        served = client.post("/analyze", json=body)
        assert served.status_code == 200, served.text
        service = served.json()

        rust = _seam("throughput_accounting_json", body)

        for key in (
            "total_throughput",
            "operating_expenses",
            "net_profit",
            "roi",
            "constraint_utilization",
        ):
            assert service[key] == rust[key], key
        assert service["product_ranking"] == rust["product_ranking"]
        assert service["optimal_mix"] == rust["optimal_mix"]


class TestLimitingFactorAndOrderAcceptance:
    @pytest.mark.parametrize("engine", ["python", "rust"])
    @pytest.mark.parametrize(
        "products",
        [
            [
                {
                    "product_id": "A",
                    "product_name": "Alpha",
                    "demand": 100,
                    "contribution_per_unit": 24,
                    "factor_per_unit": 4,
                },
                {
                    "product_id": "B",
                    "product_name": "Beta",
                    "demand": 80,
                    "contribution_per_unit": 15,
                    "factor_per_unit": 1,
                },
                {
                    "product_id": "C",
                    "product_name": "Gamma",
                    "demand": 40,
                    "contribution_per_unit": 30,
                    "factor_per_unit": 0,
                },
            ],
            [
                {
                    "product_id": "S",
                    "product_name": "Solo",
                    "demand": 12.5,
                    "contribution_per_unit": 8.4,
                    "factor_per_unit": 2.5,
                },
            ],
        ],
    )
    def test_limiting_factor_matches_the_service(self, monkeypatch, engine, products):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        client = TestClient(load_service("limiting-factor-service").main.app)
        assert client.get("/health").json()["calculation_engine"] == engine

        params = [
            ("limiting_factor", "machine hours"),
            ("factor_type", "capacity"),
            ("total_available", "160"),
        ]
        served = client.post("/analyze", params=params, json=products)
        assert served.status_code == 200, served.text
        service = served.json()

        rust = _seam(
            "limiting_factor_analyze_json",
            {
                "limiting_factor": "machine hours",
                "factor_type": "capacity",
                "total_available": 160.0,
                "products": products,
            },
        )

        for key in ("factor_used", "factor_remaining", "total_contribution"):
            assert service[key] == rust[key], key
        assert service["optimal_production_plan"] == rust["optimal_production_plan"]
        assert [
            (p["product_id"], p["ranking"], p["units_to_produce"], p["contribution_per_factor_unit"])
            for p in service["products"]
        ] == [
            (p["product_id"], p["ranking"], p["units_to_produce"], p["contribution_per_factor_unit"])
            for p in rust["products"]
        ]

    @pytest.mark.parametrize("engine", ["python", "rust"])
    def test_limiting_factor_compare_matches_the_service(self, monkeypatch, engine):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        client = TestClient(load_service("limiting-factor-service").main.app)
        products = [
            {"product_id": "A", "product_name": "Alpha", "contribution_per_unit": 24, "factor_per_unit": 4},
            {"product_id": "B", "product_name": "Beta", "contribution_per_unit": 15, "factor_per_unit": 1},
            {"product_id": "C", "product_name": "Gamma", "contribution_per_unit": 30, "factor_per_unit": 0},
        ]
        served = client.post("/compare-products", json=products)
        assert served.status_code == 200, served.text
        rust = _seam("limiting_factor_compare_json", {"products": products})
        assert served.json()["ranked_products"] == rust["ranked_products"]

    @pytest.mark.parametrize("engine", ["python", "rust"])
    @pytest.mark.parametrize(
        "params",
        [
            {
                "order_id": "ORD-1",
                "product_id": "PRD-1",
                "product_name": "Widget",
                "customer_name": "Acme",
                "normal_selling_price": 100.0,
                "offered_price": 85.0,
                "variable_cost_per_unit": 60.0,
                "units_requested": 500.0,
            },
            {
                "order_id": "ORD-2",
                "product_id": "PRD-2",
                "product_name": "Gadget",
                "customer_name": "Beta Ltd",
                "normal_selling_price": 200.0,
                "offered_price": 150.0,
                "variable_cost_per_unit": 90.0,
                "units_requested": 200.0,
                "full_cost_per_unit": 130.0,
                "incremental_fixed_costs": 4000.0,
                "displaced_sales_units": 60.0,
                "displaced_contribution_per_unit": 110.0,
            },
            {
                "order_id": "ORD-3",
                "product_id": "PRD-3",
                "product_name": "Bolt",
                "customer_name": "Gamma",
                "normal_selling_price": 50.0,
                "offered_price": 40.0,
                "variable_cost_per_unit": 45.0,
                "units_requested": 100.0,
                "displaced_sales_units": 100.0,
                "displaced_contribution_per_unit": 5.0,
            },
        ],
    )
    def test_order_acceptance_matches_the_service(self, monkeypatch, engine, params):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        module = load_service("order-acceptance-service").main
        module.decisions.clear()
        client = TestClient(module.app)
        assert client.get("/health").json()["calculation_engine"] == engine

        served = client.post("/analyze", params=params)
        assert served.status_code == 200, served.text
        service = served.json()

        payload = dict(params)
        payload.setdefault("full_cost_per_unit", None)
        rust = _seam("order_acceptance_json", payload)

        for key in (
            "price_below_normal",
            "price_below_normal_percentage",
            "contribution_per_unit_at_normal",
            "contribution_per_unit_at_offered",
            "contribution_from_order",
            "total_relevant_cost",
            "minimum_acceptable_price",
            "opportunity_cost",
            "net_contribution",
            "total_fixed_cost_incremental",
            "recommendation",
        ):
            assert service[key] == rust[key], key
        # the service model keeps a float full cost, the library reports "not given"
        assert (service["full_cost_per_unit"] or None) == rust["full_cost_per_unit"]

    @pytest.mark.parametrize("engine", ["python", "rust"])
    @pytest.mark.parametrize(
        "params",
        [
            {"product_name": "Widget", "offered_price": 90.0, "variable_cost_per_unit": 60.0, "units": 100.0},
            {
                "product_name": "Gadget",
                "offered_price": 80.0,
                "variable_cost_per_unit": 60.0,
                "units": 250.0,
                "has_spare_capacity": False,
            },
            {
                "product_name": "Bolt",
                "offered_price": 40.0,
                "variable_cost_per_unit": 45.0,
                "units": 10.0,
            },
        ],
    )
    def test_one_time_order_matches_the_service(self, monkeypatch, engine, params):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        client = TestClient(load_service("order-acceptance-service").main.app)

        served = client.post("/one-time-order", params=params)
        assert served.status_code == 200, served.text
        rust = _seam("one_time_order_json", params)
        assert served.json() == rust


class TestStandardCosting:
    CASES = [
        ("material-standard", "standard_material_json", {"standard_price_per_unit": 12.5, "standard_quantity": 480}),
        ("labour-standard", "standard_labour_json", {"standard_rate_per_hour": 18.75, "standard_hours": 32}),
        ("overhead-standard", "standard_overhead_json", {"overhead_rate": 7.4, "standard_activity_level": 900}),
        (
            "total-standard",
            "standard_total_json",
            {"standard_direct_material": 6000.0, "standard_direct_labour": 600.0, "standard_overhead": 6660.0},
        ),
        (
            "per-unit",
            "standard_per_unit_json",
            {
                "standard_material_cost": 6000.0,
                "standard_labour_cost": 600.0,
                "standard_overhead": 6660.0,
                "units_produced": 300,
            },
        ),
        (
            "per-unit",
            "standard_per_unit_json",
            {
                "standard_material_cost": 10.0,
                "standard_labour_cost": 5.0,
                "standard_overhead": 1.0,
                "units_produced": 0,
            },
        ),
        ("variance-analysis", "standard_variance_json", {"standard_cost": 13260.0, "actual_cost": 13000.0}),
        ("variance-analysis", "standard_variance_json", {"standard_cost": 900.0, "actual_cost": 940.5}),
        ("variance-analysis", "standard_variance_json", {"standard_cost": 500.0, "actual_cost": 500.0}),
    ]

    @pytest.mark.parametrize("engine", ["python", "rust"])
    @pytest.mark.parametrize("path, seam_name, params", CASES)
    def test_standard_costing_matches_the_service(self, monkeypatch, engine, path, seam_name, params):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        client = TestClient(load_service("standard-cost-service").main.app)
        assert client.get("/health").json()["calculation_engine"] == engine

        served = client.post(f"/{path}", params=params)
        assert served.status_code == 200, served.text
        assert served.json() == _seam(seam_name, params)


class TestActivityBasedCosting:
    BODY = {
        "company_id": "comp-9",
        "activities": [
            {
                "activity_id": "machining",
                "activity_name": "Machining",
                "activity_type": "unit_level",
                "cost_pool": 10000,
                "cost_driver": "machine_hours",
                "driver_volume": 1000,
            },
            {
                "activity_id": "setup",
                "activity_name": "Setup",
                "activity_type": "batch_level",
                "cost_pool": 5000,
                "cost_driver": "batches",
                "driver_volume": 50,
            },
            {
                "activity_id": "design",
                "activity_name": "Design",
                "activity_type": "product_level",
                "cost_pool": 2000,
                "cost_driver": "designs",
                "driver_volume": 0,
            },
        ],
        "products": [
            {
                "product_id": "p1",
                "product_name": "Widget",
                "unit_level_drivers": {"machining": 120},
                "batch_level_drivers": {"setup": 4},
                "product_level_activities": {"design": 250.0},
            },
            {
                "product_id": "p2",
                "product_name": "Gadget",
                "unit_level_drivers": {"machining": 45, "idle": 7},
                "batch_level_drivers": {},
                "product_level_activities": {},
            },
        ],
    }

    @pytest.mark.parametrize("engine", ["python", "rust"])
    def test_abc_matches_the_service(self, monkeypatch, engine):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        client = TestClient(load_service("activity-based-costing-service").main.app)
        assert client.get("/").json()["calculation_engine"] == engine

        served = client.post("/calculate", json=self.BODY)
        assert served.status_code == 200, served.text
        service = served.json()

        rust = _seam("abc_calculate_json", self.BODY)

        assert service["activity_rates"] == rust["activity_rates"]
        assert service["product_costs"] == rust["product_costs"]
        assert service["unit_costs"] == rust["unit_costs"]
        assert service["total_overhead_allocated"] == rust["total_overhead_allocated"]


class TestBudgetedAndActualCosts:
    BUDGET_CASES = [
        (
            "create",
            "budget_create_json",
            {
                "department": "Assembly",
                "period": "2026-11",
                "direct_material": 12000.0,
                "direct_labour": 4500.0,
                "overhead": 3200.5,
                "other_costs": 300.0,
            },
        ),
        (
            "flexible-budget",
            "budget_flexible_json",
            {"budgeted_cost_per_unit": 42.5, "actual_output": 1800, "budgeted_output": 1600},
        ),
        (
            "compare-output",
            "budget_compare_output_json",
            {"budgeted_units": 1600, "actual_units": 1800, "budgeted_cost_per_unit": 42.5},
        ),
        (
            "cost-breakdown",
            "budget_breakdown_json",
            {
                "total_budgeted_cost": 20000.0,
                "material_percent": 50,
                "labour_percent": 30,
                "overhead_percent": 20,
            },
        ),
        (
            "cost-breakdown",
            "budget_breakdown_json",
            {
                "total_budgeted_cost": 20000.0,
                "material_percent": 50,
                "labour_percent": 30,
                "overhead_percent": 10,
            },
        ),
    ]

    ACTUAL_CASES = [
        ("material-actual", "actual_material_json", {"actual_quantity": 480, "actual_price_per_unit": 12.75}),
        ("labour-actual", "actual_labour_json", {"actual_hours": 320, "actual_rate_per_hour": 18.4}),
        (
            "total-actual",
            "actual_total_json",
            {"actual_direct_material": 6120.0, "actual_direct_labour": 5888.0, "actual_overhead": 3200.0},
        ),
        ("compare-to-budget", "actual_compare_budget_json", {"actual_cost": 15208.0, "budgeted_cost": 16000.0}),
        ("compare-to-budget", "actual_compare_budget_json", {"actual_cost": 16000.0, "budgeted_cost": 16000.0}),
        ("compare-to-budget", "actual_compare_budget_json", {"actual_cost": 16500.0, "budgeted_cost": 0}),
        (
            "variance-breakdown",
            "actual_variance_json",
            {
                "actual_material": 6120.0,
                "budgeted_material": 6000.0,
                "actual_labour": 5888.0,
                "budgeted_labour": 6000.0,
                "actual_overhead": 3200.0,
                "budgeted_overhead": 4000.0,
            },
        ),
    ]

    @pytest.mark.parametrize("engine", ["python", "rust"])
    @pytest.mark.parametrize("path, seam_name, params", BUDGET_CASES)
    def test_budgeted_cost_matches_the_service(self, monkeypatch, engine, path, seam_name, params):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        client = TestClient(load_service("budgeted-cost-service").main.app)
        assert client.get("/health").json()["calculation_engine"] == engine

        served = client.post(f"/{path}", params=params)
        assert served.status_code == 200, served.text
        assert served.json() == _seam(seam_name, params)

    @pytest.mark.parametrize("engine", ["python", "rust"])
    def test_total_budget_matches_the_service(self, monkeypatch, engine):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        client = TestClient(load_service("budgeted-cost-service").main.app)
        items = [{"item": "material", "amount": 12000}, {"item": "labour", "amount": 4500}, {"item": "overhead"}]
        served = client.post("/total-budget", json=items)
        assert served.status_code == 200, served.text
        assert served.json() == _seam("budget_total_json", {"budgeted_costs": items})

    @pytest.mark.parametrize("engine", ["python", "rust"])
    @pytest.mark.parametrize("path, seam_name, params", ACTUAL_CASES)
    def test_actual_cost_matches_the_service(self, monkeypatch, engine, path, seam_name, params):
        monkeypatch.setenv("VIMBAI_CALC_ENGINE", engine)
        client = TestClient(load_service("actual-cost-service").main.app)
        assert client.get("/health").json()["calculation_engine"] == engine

        served = client.post(f"/{path}", params=params)
        assert served.status_code == 200, served.text
        assert served.json() == _seam(seam_name, params)
