"""
Integration tests for Balance Sheet, Cash Flow Statement, Cost Accounting,
Expense Tracking, Revenue Recognition, and Tax services.
"""

from datetime import datetime, timezone

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service


@pytest.fixture
def balance_sheet_client():
    app = load_service("balance-sheet-service").main.app
    return TestClient(app)


@pytest.fixture
def cash_flow_client():
    app = load_service("cash-flow-statement-service").main.app
    return TestClient(app)


@pytest.fixture
def cost_accounting_client():
    app = load_service("cost-accounting-service").main.app
    return TestClient(app)


@pytest.fixture
def expense_client():
    app = load_service("expense-tracking-service").main.app
    return TestClient(app)


@pytest.fixture
def revenue_client():
    app = load_service("revenue-recognition-service").main.app
    return TestClient(app)


@pytest.fixture
def corporate_tax_client():
    app = load_service("corporate-tax-service").main.app
    return TestClient(app)


@pytest.fixture
def calc_engine_client():
    app = load_service("realtime-calculation-engine").main.app
    return TestClient(app)


class TestBalanceSheet:
    def test_health(self, balance_sheet_client):
        assert balance_sheet_client.get("/").status_code == 200

    def test_generate_balanced_sheet(self, balance_sheet_client):
        resp = balance_sheet_client.post(
            "/generate",
            json={
                "company_id": "comp-1",
                "assets": [
                    {"name": "Cash", "amount": 100000, "category": "current", "is_liquid": True},
                    {"name": "Equipment", "amount": 50000, "category": "non_current", "is_liquid": False},
                ],
                "liabilities": [
                    {"name": "Accounts Payable", "amount": 30000, "category": "current"},
                    {"name": "Long-term Loan", "amount": 50000, "category": "non_current"},
                ],
                "equity": [
                    {"name": "Share Capital", "amount": 50000},
                    {"name": "Retained Earnings", "amount": 20000},
                ],
            },
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["total_assets"] == 150000
        assert data["total_liabilities"] == 80000
        assert data["total_equity"] == 70000
        assert data["is_balanced"] == True

    def test_generate_unbalanced_sheet(self, balance_sheet_client):
        resp = balance_sheet_client.post(
            "/generate",
            json={
                "company_id": "comp-2",
                "assets": [{"name": "Cash", "amount": 100000}],
                "liabilities": [{"name": "Loan", "amount": 50000}],
                "equity": [{"name": "Capital", "amount": 40000}],
            },
        )
        assert resp.json()["is_balanced"] == False

    def test_ratios(self, balance_sheet_client):
        balance_sheet_client.post(
            "/generate",
            json={
                "company_id": "comp-ratios",
                "assets": [
                    {"name": "Cash", "amount": 50000, "category": "current", "is_liquid": True},
                    {"name": "Inventory", "amount": 30000, "category": "current"},
                    {"name": "Equipment", "amount": 20000, "category": "non_current"},
                ],
                "liabilities": [
                    {"name": "AP", "amount": 20000, "category": "current"},
                    {"name": "Loan", "amount": 30000, "category": "non_current"},
                ],
                "equity": [{"name": "Capital", "amount": 50000}],
            },
        )
        resp = balance_sheet_client.get("/ratios/comp-ratios")
        assert resp.status_code == 200
        data = resp.json()
        assert data["current_ratio"] == 4.0  # 80000 / 20000
        assert data["debt_to_equity"] == 1.0  # 50000 / 50000


class TestCashFlowStatement:
    def test_health(self, cash_flow_client):
        assert cash_flow_client.get("/").status_code == 200

    def test_generate_statement(self, cash_flow_client):
        resp = cash_flow_client.post(
            "/generate",
            json={
                "company_id": "comp-1",
                "period_start": "2026-01-01T00:00:00Z",
                "period_end": "2026-03-31T00:00:00Z",
                "method": "indirect",
                "operating_activities": [
                    {"description": "Cash from customers", "amount": 100000, "is_inflow": True},
                    {"description": "Cash to suppliers", "amount": 60000, "is_inflow": False},
                ],
                "investing_activities": [
                    {"description": "Equipment purchase", "amount": 20000, "is_inflow": False},
                ],
                "financing_activities": [
                    {"description": "Loan proceeds", "amount": 50000, "is_inflow": True},
                    {"description": "Dividend paid", "amount": 10000, "is_inflow": False},
                ],
                "beginning_cash": 30000,
            },
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["net_operating"] == 40000
        assert data["net_investing"] == -20000
        assert data["net_financing"] == 40000
        assert data["net_change"] == 60000
        assert data["ending_cash"] == 90000


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


class TestExpenseTracking:
    def test_health(self, expense_client):
        assert expense_client.get("/").status_code == 200

    def test_create_and_approve_expense(self, expense_client):
        create = expense_client.post(
            "/expenses",
            json={
                "company_id": "comp-1",
                "employee_id": "emp-1",
                "category": "travel",
                "amount": 1500,
                "description": "Client visit",
                "vendor": "Airline",
            },
        )
        assert create.status_code == 200
        expense_id = create.json()["id"]
        approve = expense_client.put(f"/expenses/{expense_id}/approve?approver=manager-1")
        assert approve.status_code == 200
        assert approve.json()["status"] == "approved"

    def test_expense_summary(self, expense_client):
        expense_client.post(
            "/expenses", json={"company_id": "comp-sum", "employee_id": "e1", "category": "travel", "amount": 500}
        )
        expense_client.post(
            "/expenses", json={"company_id": "comp-sum", "employee_id": "e2", "category": "office", "amount": 300}
        )
        resp = expense_client.get("/summary/comp-sum")
        assert resp.status_code == 200
        data = resp.json()
        assert data["total_amount"] == 800
        assert "travel" in data["by_category"]
        assert "office" in data["by_category"]


class TestRevenueRecognition:
    def test_health(self, revenue_client):
        assert revenue_client.get("/").status_code == 200

    def test_create_contract_and_recognize(self, revenue_client):
        resp = revenue_client.post(
            "/contracts",
            json={
                "company_id": "comp-1",
                "customer_name": "Customer A",
                "obligations": [
                    {
                        "description": "Software License",
                        "transaction_price": 80000,
                        "standalone_selling_price": 80000,
                        "recognition_method": "point_in_time",
                    },
                    {
                        "description": "Implementation",
                        "transaction_price": 20000,
                        "standalone_selling_price": 20000,
                        "recognition_method": "over_time",
                    },
                ],
            },
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["total_transaction_price"] == 100000
        contract_id = data["id"]
        obligation_id = data["obligations"][0]["id"]
        recog = revenue_client.post(f"/contracts/{contract_id}/recognize?obligation_id={obligation_id}&amount=80000")
        assert recog.status_code == 200
        assert recog.json()["is_satisfied"] == True
        assert recog.json()["contract_total_recognized"] == 80000


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


class TestRealtimeCalculationEngine:
    def test_health(self, calc_engine_client):
        assert calc_engine_client.get("/").status_code == 200

    def test_npv(self, calc_engine_client):
        resp = calc_engine_client.post(
            "/npv", json={"initial_investment": 100000, "cash_flows": [30000, 40000, 50000, 40000], "discount_rate": 10}
        )
        assert resp.status_code == 200
        data = resp.json()
        assert isinstance(data["npv"], (int, float))
        assert data["profitable"] in (True, False)

    def test_irr(self, calc_engine_client):
        resp = calc_engine_client.post(
            "/irr", json={"initial_investment": 100000, "cash_flows": [30000, 40000, 50000, 40000]}
        )
        assert resp.status_code == 200
        assert resp.json()["irr"] > 0

    def test_depreciation_straight_line(self, calc_engine_client):
        resp = calc_engine_client.post(
            "/depreciation", json={"cost": 50000, "salvage_value": 5000, "useful_life": 5, "method": "straight_line"}
        )
        assert resp.status_code == 200
        data = resp.json()
        assert len(data["schedule"]) == 5
        assert data["schedule"][0]["depreciation"] == 9000  # (50000-5000)/5

    def test_depreciation_declining_balance(self, calc_engine_client):
        resp = calc_engine_client.post(
            "/depreciation",
            json={"cost": 50000, "salvage_value": 5000, "useful_life": 5, "method": "declining_balance"},
        )
        assert resp.status_code == 200
        assert len(resp.json()["schedule"]) == 5

    def test_amortization(self, calc_engine_client):
        resp = calc_engine_client.post("/amortize?principal=200000&annual_rate=6.0&years=30")
        assert resp.status_code == 200
        data = resp.json()
        assert data["monthly_payment"] > 0
        assert len(data["schedule"]) == 360  # 30 years * 12 months
