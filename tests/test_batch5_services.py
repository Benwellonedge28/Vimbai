"""
Integration tests for batch 5 of upgraded services.
"""
import pytest, importlib, sys, os
from fastapi.testclient import TestClient

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, REPO_ROOT)

def load_app(service_dir):
    main_path = os.path.join(REPO_ROOT, service_dir, "main.py")
    spec = importlib.util.spec_from_file_location(f"{service_dir}.main", main_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.app


class TestCorporateTaxService:
    def setup_method(self):
        self.client = TestClient(load_app("corporate-tax-service"))
    
    def test_tax_calculation(self):
        resp = self.client.post("/calculate", json={
            "company_id": "comp-1", "fiscal_year": 2026,
            "accounting_profit": 500000, "statutory_rate": 0.25,
            "adjustments": [
                {"description": "Depreciation diff", "amount": 20000, "type": "addition"},
                {"description": "Tax-exempt income", "amount": 10000, "type": "deduction"}
            ],
            "credits": [{"description": "R&D Credit", "amount": 15000}],
            "estimated_payments": 100000
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["taxable_income"] == 510000  # 500000 + 20000 - 10000
        assert data["tax_before_credits"] == 127500
        assert data["net_tax_liability"] == 112500
        assert data["balance_due"] == 12500
        assert len(data["installment_schedule"]) == 4


class TestLabourEfficiencyVarianceService:
    def setup_method(self):
        self.client = TestClient(load_app("labour-efficiency-variance-service"))
    
    def test_variance(self):
        resp = self.client.post("/calculate", json={
            "company_id": "comp-1", "department": "Assembly", "period": "2026-01",
            "standard_hours": 1000, "standard_rate": 20,
            "actual_hours": 1100, "actual_rate": 22,
            "actual_output": 950, "standard_hours_per_unit": 1.0,
            "idle_hours": 50
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["rate_variance"] < 0  # paying more than standard
        assert data["idle_time_variance"] < 0
        assert len(data["analysis"]) >= 2


class TestSalesVolumeVarianceService:
    def setup_method(self):
        self.client = TestClient(load_app("sales-volume-variance-service"))
    
    def test_volume_variance(self):
        resp = self.client.post("/calculate", json={
            "company_id": "comp-1", "period": "2026-Q1",
            "products": [
                {"product": "A", "budgeted_volume": 1000, "actual_volume": 1200,
                 "budgeted_price": 50, "budgeted_cost": 30},
                {"product": "B", "budgeted_volume": 500, "actual_volume": 400,
                 "budgeted_price": 80, "budgeted_cost": 50}
            ]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert len(data["product_details"]) == 2
        assert data["total_variance"] != 0


class TestCostAccountingService:
    def setup_method(self):
        self.client = TestClient(load_app("cost-accounting-service"))
    
    def test_job_cost(self):
        resp = self.client.post("/job-cost", json={
            "company_id": "comp-1", "job_id": "JOB-001", "job_name": "Custom Build",
            "direct_materials": 5000, "direct_labour": 3000,
            "direct_labour_hours": 150, "overhead_rate": 25,
            "additional_costs": [
                {"description": "Special tooling", "amount": 500, "type": "overhead"}
            ],
            "units_produced": 10
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["applied_overhead"] == 3750  # 150 * 25
        assert data["total_cost"] == 12250  # 5000 + 3000 + 3750 + 500
        assert data["cost_per_unit"] == 1225


class TestHouseholdFinanceService:
    def setup_method(self):
        self.client = TestClient(load_app("household-finance-service"))
    
    def test_household_analysis(self):
        resp = self.client.post("/analyze", json={
            "household_id": "hh-1", "period": "2026-01",
            "incomes": [
                {"source": "Salary", "amount": 3000, "frequency": "monthly"},
                {"source": "Side business", "amount": 500, "frequency": "monthly"}
            ],
            "expenses": [
                {"category": "housing", "description": "Rent", "amount": 800},
                {"category": "food", "description": "Groceries", "amount": 400},
                {"category": "savings", "description": "Savings", "amount": 500}
            ],
            "assets": [{"name": "Savings account", "value": 10000, "type": "cash"}],
            "liabilities": [{"name": "Car loan", "amount": 5000, "type": "loan"}]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_income"] == 3500
        assert data["total_expenses"] == 1700
        assert data["surplus_deficit"] == 1800
        assert data["savings_rate"] > 50
        assert data["net_worth"] == 5000
        assert data["budget_health"] in ("excellent", "good", "fair", "poor")


class TestProjectAppraisalService:
    def setup_method(self):
        self.client = TestClient(load_app("project-appraisal-service"))
    
    def test_appraisal(self):
        resp = self.client.post("/appraise", json={
            "company_id": "comp-1", "project_name": "Factory Expansion",
            "initial_investment": 200000,
            "cash_flows": [60000, 70000, 80000, 50000],
            "discount_rate": 0.10, "salvage_value": 20000
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["npv"] > 0  # profitable project
        assert data["irr"] > 10  # above discount rate
        assert data["payback_period"] > 0
        assert data["profitability_index"] > 1
        assert "Accept" in data["recommendation"]
        assert len(data["cash_flow_analysis"]) == 4


class TestGoodwillService:
    def setup_method(self):
        self.client = TestClient(load_app("goodwill-service"))
    
    def test_no_impairment(self):
        resp = self.client.post("/test", json={
            "company_id": "comp-1", "fiscal_year": 2026,
            "cgu": {"name": "Retail Division", "carrying_value": 500000,
                    "goodwill_allocated": 100000, "fair_value": 600000, "value_in_use": 550000},
            "test_method": "higher_of"
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["impairment_loss"] == 0
        assert data["is_impaired"] is False
    
    def test_impairment(self):
        resp = self.client.post("/test", json={
            "company_id": "comp-1", "fiscal_year": 2026,
            "cgu": {"name": "Mining Division", "carrying_value": 500000,
                    "goodwill_allocated": 80000, "fair_value": 400000, "value_in_use": 420000},
            "test_method": "higher_of"
        })
        data = resp.json()
        assert data["impairment_loss"] == 80000
        assert data["goodwill_impaired"] == 80000
        assert data["is_impaired"] is True


class TestMarketRiskService:
    def setup_method(self):
        self.client = TestClient(load_app("market-risk-service"))
    
    def test_var(self):
        resp = self.client.post("/var", json={
            "company_id": "comp-1", "portfolio_name": "Trading Book",
            "positions": [
                {"instrument": "USD Bond", "exposure": 100000, "volatility": 0.05},
                {"instrument": "Equities", "exposure": 50000, "volatility": 0.20},
                {"instrument": "FX Forward", "exposure": 30000, "volatility": 0.10}
            ],
            "confidence_level": 0.95, "holding_period_days": 1
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_exposure"] == 180000
        assert data["var_95"] > 0
        assert data["var_99"] > data["var_95"]
        assert data["risk_level"] in ("low", "medium", "high")


class TestRegulatoryReportingService:
    def setup_method(self):
        self.client = TestClient(load_app("regulatory-reporting-service"))
    
    def test_prudential_report(self):
        resp = self.client.post("/generate", json={
            "company_id": "comp-1", "report_type": "prudential",
            "period": "2026-Q2", "jurisdiction": "ZW",
            "data": {"capital_ratio": 14.5, "tier1_ratio": 10.2, "total_assets": 50000000}
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["status"] == "ready_for_submission"
        assert "REG-ZW" in data["filing_reference"]
        assert len(data["validation_checks"]) >= 2
    
    def test_failed_validation(self):
        resp = self.client.post("/generate", json={
            "company_id": "comp-1", "report_type": "liquidity",
            "period": "2026-Q2", "jurisdiction": "ZW",
            "data": {"liquidity_ratio": 85}
        })
        data = resp.json()
        assert data["status"] == "validation_failed"


class TestGovernmentGrantsService:
    def setup_method(self):
        self.client = TestClient(load_app("government-grants-service"))
    
    def test_asset_grant(self):
        resp = self.client.post("/recognize", json={
            "company_id": "comp-1", "grant_name": "Industrial Development Grant",
            "granting_authority": "Ministry of Industry",
            "grant_amount": 100000, "grant_type": "asset",
            "recognition_method": "deferred_income",
            "asset_useful_life": 5,
            "conditions": ["Maintain operations for 5 years", "Create 50 jobs"]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["annual_recognition"] == 20000
        assert data["deferred_income_balance"] == 80000
        assert len(data["conditions"]) == 2
    
    def test_income_grant(self):
        resp = self.client.post("/recognize", json={
            "company_id": "comp-1", "grant_name": "COVID Relief",
            "granting_authority": "Treasury",
            "grant_amount": 50000, "grant_type": "income",
            "recognition_method": "deferred_income"
        })
        data = resp.json()
        assert data["annual_recognition"] == 50000
        assert data["deferred_income_balance"] == 0
