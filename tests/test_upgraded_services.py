"""
Integration tests for the upgraded partial services.
"""

import importlib
import os
import sys

import pytest
from fastapi.testclient import TestClient

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, REPO_ROOT)


def load_app(service_dir):
    from tests.conftest import service_missing

    if service_missing(service_dir):
        import pytest

        pytest.skip(f"{service_dir} has been deleted; its maths lives in the Rust library")
    main_path = os.path.join(REPO_ROOT, service_dir, "main.py")
    spec = importlib.util.spec_from_file_location(f"{service_dir}.main", main_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.app


class TestAutomationEngineService:
    def setup_method(self):
        import os as _os

        app = load_app("automation-engine-service")
        svc_dir = _os.path.join(
            _os.path.dirname(_os.path.dirname(_os.path.abspath(__file__))), "automation-engine-service"
        )
        spec = importlib.util.spec_from_file_location("ae_upgraded_fake", _os.path.join(svc_dir, "fake_neo4j.py"))
        fake_mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fake_mod)
        self._session = fake_mod.FakeSession()

        import automation_engine_service.database as db

        db.Neo4jConnector.get_driver = classmethod(lambda cls: fake_mod.FakeDriver(self._session))
        self.client = TestClient(app)
        self.client.headers.update({"X-User-Id": "root-ae-user"})

    def test_health(self):
        assert self.client.get("/health").status_code == 200

    def test_create_and_list_rule(self):
        resp = self.client.post(
            "/rules",
            json={
                "name": "Auto-reconcile",
                "company_id": "comp-1",
                "trigger": "scheduled",
                "steps": [{"step_id": "s1", "step_name": "Fetch", "action": "GET /transactions", "params": {}}],
            },
        )
        assert resp.status_code == 200
        rule_id = resp.json()["id"]
        assert rule_id

        resp = self.client.get("/rules", params={"company_id": "comp-1"})
        assert len(resp.json()) >= 1

    def test_execute_rule(self):
        create = self.client.post(
            "/rules",
            json={
                "name": "Test Rule",
                "company_id": "comp-1",
                "trigger": "manual",
                "steps": [{"step_id": "s1", "step_name": "Step 1", "action": "do_something"}],
            },
        )
        rule_id = create.json()["id"]
        resp = self.client.post(f"/execute/{rule_id}")
        assert resp.status_code == 200
        assert resp.json()["status"] in ("running", "completed")


class TestSupplyChainService:
    def setup_method(self):
        import importlib.util
        import os as _os

        app = load_app("supply-chain-service")
        svc_dir = _os.path.join(_os.path.dirname(_os.path.dirname(_os.path.abspath(__file__))), "supply-chain-service")
        spec = importlib.util.spec_from_file_location("sc_upgraded_fake", _os.path.join(svc_dir, "fake_neo4j.py"))
        fake_mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fake_mod)
        self._session = fake_mod.FakeSession()

        import supply_chain_service.database as db

        db.Neo4jConnector.get_driver = classmethod(lambda cls: fake_mod.FakeDriver(self._session))
        self.client = TestClient(app)
        self.client.headers.update({"X-User-Id": "upgraded-sc-user"})

    def test_supplier_crud(self):
        resp = self.client.post("/suppliers", json={"name": "Acme Corp", "lead_time_days": 7, "rating": 4.5})
        assert resp.status_code == 200
        assert resp.json()["name"] == "Acme Corp"

        resp = self.client.get("/suppliers")
        assert len(resp.json()) >= 1

    def test_inventory_and_low_stock(self):
        self.client.post(
            "/inventory",
            json={
                "sku": "WIDGET-001",
                "name": "Widget",
                "company_id": "comp-1",
                "quantity": 5,
                "reorder_point": 10,
                "reorder_qty": 50,
            },
        )
        resp = self.client.get("/inventory/low-stock", params={"company_id": "comp-1"})
        data = resp.json()
        assert len(data) >= 1
        assert data[0]["urgency"] in ("critical", "warning")

    def test_demand_forecast(self):
        resp = self.client.post(
            "/forecast",
            json={
                "sku": "PROD-001",
                "company_id": "comp-1",
                "historical_data": [100, 110, 105, 120, 115],
                "forecast_periods": 3,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert len(data["forecast"]) == 3
        assert data["method"] == "moving_average_with_trend"

    def test_purchase_order_flow(self):
        po = self.client.post(
            "/purchase-orders",
            json={
                "company_id": "comp-1",
                "supplier_id": "sup-1",
                "item_sku": "WIDGET-001",
                "quantity": 100,
                "unit_cost": 5.0,
            },
        )
        po_id = po.json()["id"]
        receive = self.client.post(f"/purchase-orders/{po_id}/receive", params={"company_id": "comp-1"})
        assert receive.status_code == 200
        assert receive.json()["status"] == "received"


class TestInventoryValuationService:
    def setup_method(self):
        self.client = TestClient(load_app("inventory-valuation-service"))

    def test_fifo(self):
        resp = self.client.post(
            "/calculate",
            json={
                "company_id": "comp-1",
                "method": "fifo",
                "purchases": [
                    {"date": "2026-01-01", "quantity": 100, "unit_cost": 10},
                    {"date": "2026-02-01", "quantity": 50, "unit_cost": 12},
                ],
                "sales": [{"date": "2026-03-01", "quantity": 80, "unit_price": 15}],
                "opening_inventory": 0,
                "opening_qty": 0,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["cost_of_goods_sold"] == 800  # 80 * 10 (FIFO)
        assert data["ending_qty"] == 70  # 150 - 80

    def test_weighted_average(self):
        resp = self.client.post(
            "/calculate",
            json={
                "company_id": "comp-1",
                "method": "weighted_average",
                "purchases": [
                    {"date": "2026-01-01", "quantity": 100, "unit_cost": 10},
                    {"date": "2026-02-01", "quantity": 50, "unit_cost": 12},
                ],
                "sales": [{"date": "2026-03-01", "quantity": 80, "unit_price": 15}],
                "opening_inventory": 0,
                "opening_qty": 0,
            },
        )
        data = resp.json()
        avg_cost = (1000 + 600) / 150
        assert data["cost_of_goods_sold"] == round(80 * avg_cost, 2)


class TestAmortizationService:
    def setup_method(self):
        self.client = TestClient(load_app("amortization-service"))

    def test_straight_line(self):
        resp = self.client.post(
            "/calculate",
            json={
                "company_id": "comp-1",
                "intangibles": [
                    {
                        "intangible_id": "pat1",
                        "type": "patent",
                        "cost": 100000,
                        "useful_life_years": 10,
                        "residual_value": 0,
                        "method": "straight_line",
                    }
                ],
                "period_start": "2026-01-01",
                "period_end": "2026-12-31",
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_amortization"] == 10000
        assert data["schedules"][0]["annual_amortization"] == 10000
        assert data["schedules"][0]["monthly_amortization"] == round(10000 / 12, 2)


class TestMFAService:
    """mfa-auth is now Neo4j-backed: MFA is self-service (X-User-Id must match)."""

    def setup_method(self):
        import os as _os

        app = load_app("mfa-auth-service")
        svc_dir = _os.path.join(_os.path.dirname(_os.path.dirname(_os.path.abspath(__file__))), "mfa-auth-service")
        spec = importlib.util.spec_from_file_location("mfa_upgraded_fake", _os.path.join(svc_dir, "fake_neo4j.py"))
        fake_mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fake_mod)
        self._session = fake_mod.FakeSession()

        import mfa_auth_service.database as db

        db.Neo4jConnector.get_driver = classmethod(lambda cls: fake_mod.FakeDriver(self._session))
        self.client = TestClient(app)

    def test_setup_and_verify(self):
        h1 = {"X-User-Id": "user-1"}
        setup = self.client.post("/setup", json={"user_id": "user-1", "method": "totp"}, headers=h1)
        data = setup.json()
        assert setup.status_code == 200
        assert data["secret"]
        assert len(data["backup_codes"]) == 8

        verify = self.client.post("/verify", json={"user_id": "user-1", "code": "123456"}, headers=h1)
        assert verify.status_code == 200

    def test_challenge_flow(self):
        challenge = self.client.post(
            "/challenge", params={"user_id": "user-2", "method": "sms"}, headers={"X-User-Id": "user-2"}
        )
        data = challenge.json()
        assert "challenge_id" in data
        # Can't verify without knowing the code, but structure should work


class TestDebtManagementService:
    """debt-management is now Neo4j-backed with X-User-Id auth + Book scoping."""

    H = {"X-User-Id": "upg-debt-user"}

class TestScenarioAnalysisService:
    def setup_method(self):
        self.client = TestClient(load_app("scenario-analysis-service"))
        self.client.headers.update({"X-User-Id": "root-upgraded-user"})

    def test_scenario_analysis(self):
        resp = self.client.post(
            "/analyze",
            json={
                "company_id": "comp-1",
                "base_revenue": 1000000,
                "base_cost": 700000,
                "base_interest": 20000,
                "base_depreciation": 50000,
                "best_case": {
                    "revenue_growth": 0.2,
                    "cost_growth": 0.03,
                    "interest_rate": 0.05,
                    "tax_rate": 0.25,
                    "description": "Optimistic",
                },
                "base_case": {
                    "revenue_growth": 0.1,
                    "cost_growth": 0.05,
                    "interest_rate": 0.05,
                    "tax_rate": 0.25,
                    "description": "Expected",
                },
                "worst_case": {
                    "revenue_growth": -0.1,
                    "cost_growth": 0.08,
                    "interest_rate": 0.07,
                    "tax_rate": 0.25,
                    "description": "Pessimistic",
                },
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["best_case"]["net_income"] > data["worst_case"]["net_income"]
        assert len(data["recommendation"]) > 0
