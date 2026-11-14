"""
Integration tests for Balance Sheet, Cash Flow Statement, Cost Accounting,
Expense Tracking, Revenue Recognition, and Tax services.
"""

import importlib
import importlib.util
import os
import sys
from datetime import datetime, timezone
from types import ModuleType

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_H = {"X-User-Id": "root-bs-user"}


def _patch_fake(pkg_name, fake_alias):
    """Give the service's Neo4j connector a fake in-memory driver."""
    fake_path = os.path.join(_ROOT, pkg_name.replace("_", "-"), "fake_neo4j.py")
    spec = importlib.util.spec_from_file_location(fake_alias, fake_path)
    fake = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fake)

    cached = sys.modules.get(pkg_name)
    if cached is None or not hasattr(cached, "__path__"):
        pkg_mod = ModuleType(pkg_name)
        pkg_mod.__path__ = [os.path.join(_ROOT, pkg_name.replace("_", "-"))]
        sys.modules[pkg_name] = pkg_mod

    db = importlib.import_module(f"{pkg_name}.database")
    session = fake.FakeSession()
    db.Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(session))


@pytest.fixture
def balance_sheet_client():
    _patch_fake("balance_sheet_service", "balance_sheet_root_fake")
    app = load_service("balance-sheet-service").main.app
    return TestClient(app)


@pytest.fixture
def cash_flow_client():
    _patch_fake("cash_flow_statement_service", "cash_flow_root_fake")
    app = load_service("cash-flow-statement-service").main.app
    return TestClient(app)


@pytest.fixture
def cost_accounting_client():
    app = load_service("cost-accounting-service").main.app
    return TestClient(app)


@pytest.fixture
def revenue_client():
    _patch_fake("revenue_recognition_service", "revenue_root_fake")
    app = load_service("revenue-recognition-service").main.app
    client = TestClient(app)
    client.headers.update(_H)
    return client


@pytest.fixture
def corporate_tax_client():
    app = load_service("corporate-tax-service").main.app
    return TestClient(app)


class TestCostAccounting:
    def test_health(self, cost_accounting_client):
        assert cost_accounting_client.get("/").status_code == 200

    def test_standard_costing(self, cost_accounting_client):
        resp = cost_accounting_client.post(
            "/standards",
            json={
                "company_id": "comp-1",
                "product_name": "Widget A",
                "direct_materials_std": 10,
                "direct_labor_std": 5,
                "overhead_std": 3,
                "units_produced": 1000,
                "actual_materials": 12000,
                "actual_labor": 4500,
                "actual_overhead": 3500,
            },
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["standard_cost_per_unit"] == 18  # 10+5+3
        assert data["actual_cost_per_unit"] == 20.0  # 20000/1000
        assert data["material_variance"] == 2000  # 12000 - 10000
        assert data["total_variance"] == 2000  # 2000 + (-500) + 500


class TestCorporateTax:
    def test_health(self, corporate_tax_client):
        assert corporate_tax_client.get("/").status_code == 200

    def test_compute_tax(self, corporate_tax_client):
        resp = corporate_tax_client.post(
            "/compute",
            json={
                "company_id": "comp-1",
                "tax_year": 2026,
                "revenue": 1000000,
                "deductible_expenses": 600000,
                "capital_allowances": 100000,
                "tax_rate": 25.0,
                "credits": 5000,
            },
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["taxable_income"] == 300000  # 1M - 600K - 100K
        assert data["tax_owed"] == 75000  # 300K * 25%
        assert data["net_tax_liability"] == 70000  # 75000 - 5000

    def test_provisional_tax(self, corporate_tax_client):
        resp = corporate_tax_client.post("/provision/comp-1?tax_year=2026&annual_estimate=2000000&tax_rate=25.0")
        assert resp.status_code == 200
        data = resp.json()
        assert data["quarterly_payment"] == 125000  # 2M * 25% / 4
        assert len(data["due_dates"]) == 4
