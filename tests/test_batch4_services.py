"""
Integration tests for batch 4 of upgraded services.
"""

import importlib
import os
import sys

import pytest
from fastapi.testclient import TestClient

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, REPO_ROOT)


def load_app(service_dir):
    main_path = os.path.join(REPO_ROOT, service_dir, "main.py")
    spec = importlib.util.spec_from_file_location(f"{service_dir}.main", main_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.app


class TestBusinessCombinationService:
    def setup_method(self):
        self.client = TestClient(load_app("business-combination-service"))

    def test_acquisition(self):
        resp = self.client.post(
            "/acquire",
            json={
                "company_id": "comp-1",
                "acquirer": "Parent Co",
                "acquiree": "Sub Co",
                "purchase_price": 500000,
                "identifiable_assets": [
                    {"name": "Property", "fair_value": 200000, "type": "tangible"},
                    {"name": "Brand", "fair_value": 50000, "type": "intangible"},
                    {"name": "Bank Loan", "fair_value": -30000, "type": "liability"},
                ],
                "contingent_consideration": 20000,
                "acquisition_costs": 5000,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_identifiable_assets"] == 250000
        assert data["total_identifiable_liabilities"] == 30000
        assert data["net_identifiable_assets"] == 220000
        # Goodwill = 500000 + 20000 - 220000 = 300000
        assert data["goodwill"] == 300000
        assert len(data["purchase_price_allocation"]) >= 3


class TestDivestitureService:
    def setup_method(self):
        self.client = TestClient(load_app("divestiture-service"))

    def test_disposal(self):
        resp = self.client.post(
            "/dispose",
            json={
                "company_id": "comp-1",
                "subsidiary_name": "Sub Co",
                "carrying_value": 300000,
                "disposal_proceeds": 400000,
                "disposal_costs": 10000,
                "associated_goodwill": 50000,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["net_proceeds"] == 390000
        assert data["gain_or_loss"] > 0
        assert len(data["journal_entries"]) > 0


class TestForeignCurrencyTranslationService:
    def setup_method(self):
        self.client = TestClient(load_app("foreign-currency-translation-service"))

    def test_translation(self):
        resp = self.client.post(
            "/translate",
            json={
                "company_id": "comp-1",
                "subsidiary": "Zambia Sub",
                "functional_currency": "ZMW",
                "presentation_currency": "USD",
                "closing_rate": 0.05,
                "avg_rate": 0.048,
                "historical_rate": 0.055,
                "net_assets": [
                    {"account": "Cash", "amount": 1000000, "is_monetary": True},
                    {"account": "Property", "amount": 5000000, "is_monetary": False},
                ],
                "income_statement": [{"account": "Revenue", "amount": 2000000, "is_monetary": True}],
                "goodwill": 500000,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["translated_net_assets"] > 0
        assert data["translated_income"] > 0
        assert len(data["details"]) == 3


class TestAnalyticsService:
    def setup_method(self):
        self.client = TestClient(load_app("analytics-service"))

    def test_full_analysis(self):
        resp = self.client.post(
            "/analyze",
            json={
                "company_id": "comp-1",
                "period": "2026",
                "financials": {
                    "revenue": 2000000,
                    "net_income": 300000,
                    "total_assets": 1500000,
                    "current_assets": 500000,
                    "current_liabilities": 300000,
                    "total_liabilities": 600000,
                    "total_equity": 900000,
                    "inventory": 100000,
                    "accounts_receivable": 200000,
                    "cogs": 1200000,
                    "operating_cash_flow": 400000,
                    "shares_outstanding": 10000,
                },
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["profitability"]["roe"] > 0
        assert data["liquidity"]["current_ratio"] > 1
        assert data["solvency"]["debt_to_equity"] < 1
        assert data["overall_score"] >= 0
        assert len(data["insights"]) > 0


class TestBalancedScorecardService:
    def setup_method(self):
        self.client = TestClient(load_app("balanced-scorecard-service"))

    def test_scorecard(self):
        resp = self.client.post(
            "/score",
            json={
                "company_id": "comp-1",
                "period": "2026-Q1",
                "kpis": [
                    {"name": "Revenue Growth", "perspective": "financial", "target": 100, "actual": 110, "weight": 2.0},
                    {
                        "name": "Customer Satisfaction",
                        "perspective": "customer",
                        "target": 90,
                        "actual": 85,
                        "weight": 1.0,
                    },
                    {
                        "name": "Process Efficiency",
                        "perspective": "internal",
                        "target": 95,
                        "actual": 92,
                        "weight": 1.0,
                    },
                    {
                        "name": "Training Hours",
                        "perspective": "learning_growth",
                        "target": 40,
                        "actual": 30,
                        "weight": 1.0,
                    },
                ],
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["overall_score"] > 0
        assert data["financial_score"] > data["learning_score"]
        assert len(data["action_items"]) > 0


class TestBankFeeAnalysisService:
    def setup_method(self):
        self.client = TestClient(load_app("bank-fee-analysis-service"))

    def test_fee_analysis(self):
        resp = self.client.post(
            "/analyze",
            json={
                "company_id": "comp-1",
                "period": "2026",
                "bank_name": "Stanbic",
                "charges": [
                    {
                        "date": "2026-01-15",
                        "description": "Monthly maintenance",
                        "amount": 50,
                        "category": "maintenance",
                    },
                    {"date": "2026-01-20", "description": "Wire transfer", "amount": 25, "category": "transaction"},
                    {"date": "2026-02-01", "description": "Overdraft fee", "amount": 200, "category": "overdraft"},
                    {
                        "date": "2026-02-10",
                        "description": "FX conversion",
                        "amount": 100,
                        "category": "foreign_exchange",
                    },
                ],
                "transaction_volume": 100,
                "avg_balance": 50000,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_fees"] == 375
        assert "overdraft" in data["fee_by_category"]
        assert data["fee_per_transaction"] == 3.75
        assert len(data["recommendations"]) > 0


class TestFinancialForecastingService:
    def setup_method(self):
        self.client = TestClient(load_app("financial-forecasting-service"))

    def test_linear_forecast(self):
        resp = self.client.post(
            "/forecast",
            json={
                "company_id": "comp-1",
                "metric_name": "Revenue",
                "historical_values": [100, 110, 120, 130, 140, 150],
                "periods_ahead": 4,
                "method": "linear",
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert len(data["forecast"]) == 4
        assert data["trend"] == "increasing"
        assert data["r_squared"] > 0.9
        assert data["method_used"] == "linear"

    def test_insufficient_data(self):
        resp = self.client.post(
            "/forecast",
            json={"company_id": "comp-1", "metric_name": "Revenue", "historical_values": [100], "periods_ahead": 4},
        )
        data = resp.json()
        assert data["trend"] == "insufficient_data"


class TestPensionAccountingService:
    def setup_method(self):
        self.client = TestClient(load_app("pension-accounting-service"))

    def test_pension_calc(self):
        resp = self.client.post(
            "/calculate",
            json={
                "company_id": "comp-1",
                "fiscal_year": 2026,
                "plan_name": "Staff Pension",
                "obligation_beginning": 1000000,
                "service_cost": 50000,
                "interest_cost": 70000,
                "benefits_paid": 80000,
                "actuarial_gain_loss": -20000,
                "plan_assets_beginning": 800000,
                "contributions": 60000,
                "expected_return": 56000,
                "actual_return": 50000,
                "benefits_paid_from_assets": 80000,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["dbo_end"] > 0
        assert data["plan_assets_end"] > 0
        assert "defined_benefit_asset" in data["balance_sheet_entry"]
        assert data["total_pension_expense"] > 0


class TestCapitalAllocationService:
    def setup_method(self):
        self.client = TestClient(load_app("capital-allocation-service"))

    def test_allocation(self):
        resp = self.client.post(
            "/allocate",
            json={
                "company_id": "comp-1",
                "capital_budget": 500000,
                "projects": [
                    {
                        "name": "Project A",
                        "initial_investment": 200000,
                        "annual_cash_flows": [80000, 80000, 80000, 80000],
                        "discount_rate": 0.10,
                        "strategic_value": 8,
                    },
                    {
                        "name": "Project B",
                        "initial_investment": 300000,
                        "annual_cash_flows": [120000, 120000, 120000],
                        "discount_rate": 0.10,
                        "strategic_value": 6,
                    },
                    {
                        "name": "Project C",
                        "initial_investment": 400000,
                        "annual_cash_flows": [150000, 150000, 150000],
                        "discount_rate": 0.10,
                        "strategic_value": 9,
                    },
                ],
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert len(data["selected_projects"]) >= 1
        assert data["total_investment"] <= 500000
        assert data["utilization_pct"] > 0
        assert "npv" in data["selected_projects"][0]


class TestInternalControlsService:
    def setup_method(self):
        self.client = TestClient(load_app("internal-controls-testing-service"))

    def test_control_testing(self):
        resp = self.client.post(
            "/test",
            json={
                "company_id": "comp-1",
                "fiscal_year": 2026,
                "controls": [
                    {
                        "name": "Segregation of Duties",
                        "control_type": "preventive",
                        "description": "Segregation between authorization and custody",
                        "process": "Procurement",
                        "test_sample_size": 25,
                        "exceptions_found": 0,
                    },
                    {
                        "name": "Bank Reconciliation",
                        "control_type": "detective",
                        "description": "Monthly bank reconciliation review",
                        "process": "Treasury",
                        "test_sample_size": 12,
                        "exceptions_found": 2,
                    },
                    {
                        "name": "Access Controls",
                        "control_type": "preventive",
                        "description": "User access review",
                        "process": "IT",
                        "test_sample_size": 50,
                        "exceptions_found": 8,
                    },
                ],
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_controls"] == 3
        assert data["effective_controls"] >= 1
        assert data["deficient_controls"] >= 1
        assert "opinion" in data["overall_assessment"].lower()
        assert len(data["remediation_plan"]) >= 1


class TestRegulatoryComplianceService:
    def setup_method(self):
        self.client = TestClient(load_app("regulatory-compliance-service"))

    def test_regulation_lifecycle(self):
        reg = self.client.post(
            "/regulations",
            json={
                "company_id": "comp-1",
                "regulation_name": "IFRS 15 Revenue",
                "jurisdiction": "ZW",
                "framework": "IFRS",
                "requirement": "Recognize revenue when performance obligation satisfied",
                "status": "pending_review",
                "risk_if_non_compliant": "high",
            },
        )
        reg_id = reg.json()["id"]

        updated = self.client.post(
            f"/regulations/{reg_id}/update", params={"company_id": "comp-1", "status": "compliant"}
        )
        assert updated.json()["updated"] is True

        dashboard = self.client.get("/dashboard", params={"company_id": "comp-1"})
        data = dashboard.json()
        assert data["compliant"] >= 1
        assert "IFRS" in data["by_framework"]


class TestXBRLReportingService:
    def setup_method(self):
        self.client = TestClient(load_app("xbrl-reporting-service"))

    def test_xbrl_generation(self):
        resp = self.client.post(
            "/generate",
            json={
                "company_id": "comp-1",
                "fiscal_year": 2026,
                "entity_name": "Vimbai Ltd",
                "entity_identifier": "ZW0001",
                "taxonomy": "ifrs-full",
                "concepts": [
                    {"concept": "RevenueFromContracts", "value": 2000000},
                    {"concept": "ProfitLoss", "value": 300000},
                    {"concept": "Assets", "value": 1500000},
                    {"concept": "Liabilities", "value": 600000},
                    {"concept": "Equity", "value": 900000},
                ],
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["validation_status"] == "valid"
        assert data["concept_count"] == 5
        assert len(data["xbrl_facts"]) == 5

    def test_missing_concepts(self):
        resp = self.client.post(
            "/generate",
            json={
                "company_id": "comp-1",
                "fiscal_year": 2026,
                "entity_name": "Test Co",
                "entity_identifier": "TEST001",
                "concepts": [{"concept": "RevenueFromContracts", "value": 1000000}],
            },
        )
        data = resp.json()
        assert data["validation_status"] == "invalid"
        assert len(data["validation_errors"]) > 0


class TestSubscriptionPlansService:
    def setup_method(self):
        self.client = TestClient(load_app("subscription-plans-service"))

    def test_plan_and_subscribe(self):
        plan = self.client.post(
            "/plans",
            json={
                "tier": "professional",
                "name": "Pro Plan",
                "price_monthly": 199,
                "features": ["Multi-company", "Advanced reporting"],
                "max_users": 50,
                "max_companies": 10,
                "api_calls_per_month": 10000,
            },
        )
        plan_id = plan.json()["id"]

        sub = self.client.post("/subscribe", params={"company_id": "comp-1", "plan_id": plan_id, "cycle": "monthly"})
        data = sub.json()
        assert sub.status_code == 200
        assert data["company_id"] == "comp-1"
        assert data["status"] == "active"

    def test_upgrade(self):
        resp = self.client.post(
            "/upgrade",
            json={
                "company_id": "comp-1",
                "current_plan": "basic",
                "target_plan": "professional",
                "current_period_end": "2026-10-01T00:00:00Z",
                "prorate": True,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["current_plan"] == "basic"
        assert data["target_plan"] == "professional"
        assert data["new_billing_amount"] == 199
