"""
Integration tests for the newly implemented stub services.
Tests the real business logic endpoints of all 17 implemented services.
Uses importlib to load services with hyphenated directory names.
"""
import pytest, importlib, sys, os
from fastapi.testclient import TestClient

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, REPO_ROOT)

def load_service_app(service_dir):
    """Dynamically load a service's FastAPI app from a hyphenated directory name."""
    main_path = os.path.join(REPO_ROOT, service_dir, "main.py")
    spec = importlib.util.spec_from_file_location(f"{service_dir}.main", main_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.app


class TestTaxCalculationService:
    def setup_method(self):
        self.client = TestClient(load_service_app("tax-calculation-service"))
    
    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "tax-calculation" in resp.json()["service"]
    
    def test_income_tax_progressive(self):
        resp = self.client.post("/calculate", json={
            "company_id": "comp-1", "tax_type": "income_tax",
            "taxable_amount": 50000, "fiscal_year": 2026
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["tax_owed"] > 0
        assert data["effective_rate"] < data["marginal_rate"]
    
    def test_vat_calculation(self):
        resp = self.client.post("/calculate", json={
            "company_id": "comp-1", "tax_type": "vat",
            "taxable_amount": 1000
        })
        data = resp.json()
        assert data["tax_owed"] == 150.0
        assert data["effective_rate"] == 0.15
    
    def test_capital_gains(self):
        resp = self.client.post("/calculate/capital-gains", params={
            "company_id": "comp-1", "asset_name": "Property",
            "purchase_price": 100000, "sale_price": 150000,
            "holding_period_days": 500, "fiscal_year": 2026
        })
        data = resp.json()
        assert data["tax_owed"] == 10000.0
        assert data["breakdown"]["gain"] == 50000


class TestProfitLossService:
    def setup_method(self):
        self.client = TestClient(load_service_app("profit-loss-account-service"))
    
    def test_pnl_preparation(self):
        resp = self.client.post("/prepare", json={
            "company_id": "comp-1", "fiscal_year_start": "2026-01-01",
            "fiscal_year_end": "2026-12-31",
            "revenue": 1000000, "cost_of_goods_sold": 600000,
            "operating_expenses": 200000, "depreciation": 50000,
            "interest_expense": 20000, "tax_rate": 0.25
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["gross_profit"] == 400000
        assert data["ebitda"] == 200000
        assert data["ebit"] == 150000
        assert data["net_income"] > 0
    
    def test_pnl_comparison(self):
        resp = self.client.post("/compare", params={
            "company_id": "comp-1", "previous_revenue": 800000, "previous_net_income": 100000
        }, json={
            "company_id": "comp-1", "fiscal_year_start": "2026-01-01",
            "fiscal_year_end": "2026-12-31",
            "revenue": 1000000, "cost_of_goods_sold": 600000,
            "operating_expenses": 200000, "tax_rate": 0.25
        })
        data = resp.json()
        assert data["revenue_change_pct"] == 25.0


class TestTradingAccountService:
    def setup_method(self):
        self.client = TestClient(load_service_app("trading-account-service"))
    
    def test_trading_account(self):
        resp = self.client.post("/generate", json={
            "company_id": "comp-1", "period": "2026",
            "opening_stock": 10000, "purchases": 50000, "carriage_inward": 2000,
            "closing_stock": 15000, "sales": 100000, "sales_returns": 5000,
            "direct_wages": 8000, "factory_overhead": 5000
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["net_sales"] == 95000
        assert data["cost_of_goods_sold"] == 60000
        assert data["gross_profit"] == 35000


class TestPayrollService:
    def setup_method(self):
        self.client = TestClient(load_service_app("payroll-accounting-service"))
    
    def test_payroll_processing(self):
        resp = self.client.post("/process", json={
            "company_id": "comp-1", "period": "2026-01",
            "employees": [
                {"employee_id": "e1", "employee_name": "Alice", "gross_salary": 5000},
                {"employee_id": "e2", "employee_name": "Bob", "gross_salary": 3000}
            ]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_gross"] == 8000
        assert data["total_net"] < 8000
        assert data["employee_count"] == 2
        assert len(data["journal_entries"]) == 8
        assert data["total_cost_to_company"] > 8000


class TestPartnershipService:
    def setup_method(self):
        self.client = TestClient(load_service_app("partnership-accounting-service"))
    
    def test_profit_allocation(self):
        resp = self.client.post("/allocate", json={
            "company_id": "comp-1", "period": "2026",
            "net_profit": 200000, "interest_rate_on_capital": 0.05,
            "partners": [
                {"partner_id": "p1", "name": "Alice", "capital_contribution": 100000,
                 "profit_share_pct": 60, "salary": 30000, "drawings": 10000},
                {"partner_id": "p2", "name": "Bob", "capital_contribution": 50000,
                 "profit_share_pct": 40, "salary": 20000, "drawings": 5000}
            ]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert len(data["partner_accounts"]) == 2
        assert data["total_salary"] == 50000
        for acct in data["partner_accounts"]:
            assert acct["closing_capital"] > 0


class TestMakeOrBuyService:
    def setup_method(self):
        self.client = TestClient(load_service_app("make-or-buy-decision-service"))
    
    def test_make_cheaper(self):
        resp = self.client.post("/analyze", json={
            "company_id": "comp-1", "product_name": "Widget", "annual_volume": 10000,
            "make_costs": {"direct_materials": 5, "direct_labour": 3, "variable_overhead": 1,
                          "fixed_overhead": 5000, "setup_cost": 1000},
            "buy_costs": {"unit_purchase_price": 12, "ordering_cost": 500}
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["recommendation"] in ("MAKE", "BUY")
        assert data["difference"] != 0
        assert len(data["qualitative_factors"]) > 0


class TestSalesVarianceService:
    def setup_method(self):
        self.client = TestClient(load_service_app("sales-price-variance-service"))
    
    def test_variance_analysis(self):
        resp = self.client.post("/analyze", json={
            "company_id": "comp-1", "period": "2026-Q1",
            "items": [
                {"product_name": "A", "budgeted_price": 100, "actual_price": 110,
                 "budgeted_volume": 1000, "actual_volume": 950},
                {"product_name": "B", "budgeted_price": 50, "actual_price": 45,
                 "budgeted_volume": 2000, "actual_volume": 2100}
            ]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_sales_price_variance"] != 0
        assert len(data["items"]) == 2
        assert data["items"][0]["sales_price_variance"] > 0


class TestCacheService:
    def setup_method(self):
        self.client = TestClient(load_service_app("cache-service"))
    
    def test_set_and_get(self):
        self.client.post("/cache", json={"key": "test-key", "value": {"data": 42}, "ttl": 60})
        resp = self.client.get("/cache/test-key")
        data = resp.json()
        assert data["found"] is True
        assert data["value"]["data"] == 42
    
    def test_miss_on_missing_key(self):
        resp = self.client.get("/cache/nonexistent")
        data = resp.json()
        assert data["found"] is False
    
    def test_delete(self):
        self.client.post("/cache", json={"key": "del-key", "value": "x", "ttl": 60})
        resp = self.client.delete("/cache/del-key")
        assert resp.json()["deleted"] is True
        resp = self.client.get("/cache/del-key")
        assert resp.json()["found"] is False
    
    def test_stats(self):
        resp = self.client.get("/stats")
        data = resp.json()
        assert "cache_size" in data
        assert "hit_rate_pct" in data


class TestBusinessDocumentsService:
    def setup_method(self):
        self.client = TestClient(load_service_app("business-documents-service"))
    
    def test_invoice_generation(self):
        resp = self.client.post("/generate", json={
            "company_id": "comp-1", "document_type": "invoice",
            "company_name": "Vimbai Ltd", "recipient": "Client Co",
            "line_items": [
                {"description": "Consulting", "quantity": 10, "unit_price": 200, "tax_rate": 0.15},
                {"description": "Software License", "quantity": 1, "unit_price": 5000, "tax_rate": 0.15}
            ]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["subtotal"] == 7000
        assert data["tax_total"] == 1050
        assert data["grand_total"] == 8050
        assert len(data["line_items"]) == 2


class TestGovernmentGrantsService:
    def setup_method(self):
        self.client = TestClient(load_service_app("government-grants-service"))
    
    def test_asset_grant_recognition(self):
        resp = self.client.post("/recognize", json={
            "company_id": "comp-1", "grant_name": "Equipment Grant",
            "grant_amount": 50000, "grant_type": "asset",
            "related_asset_cost": 200000, "useful_life_years": 5,
            "conditions": ["Must remain operational for 5 years"]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["annual_amortization"] == 10000
        assert len(data["journal_entries"]) == 2
        assert len(data["disclosure_notes"]) >= 2


class TestExoticDerivativesService:
    def setup_method(self):
        self.client = TestClient(load_service_app("exotic-derivatives-service"))
    
    def test_vanilla_option(self):
        resp = self.client.post("/price", json={
            "option_type": "vanilla", "underlying": "AAPL",
            "spot_price": 150, "strike_price": 145,
            "volatility": 0.25, "risk_free_rate": 0.05, "time_to_expiry": 0.5
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["estimated_price"] > 0
        assert data["delta"] > 0 and data["delta"] < 1
        assert "Black-Scholes" in data["model_used"]
    
    def test_binary_option(self):
        resp = self.client.post("/price", json={
            "option_type": "binary", "underlying": "EURUSD",
            "spot_price": 1.10, "strike_price": 1.12,
            "volatility": 0.08, "risk_free_rate": 0.03, "time_to_expiry": 0.25
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["estimated_price"] > 0
        assert "Binary" in data["model_used"]


class TestIncomeStatementService:
    def setup_method(self):
        self.client = TestClient(load_service_app("income-statement-service"))
    
    def test_full_income_statement(self):
        resp = self.client.post("/prepare", json={
            "company_id": "comp-1", "fiscal_year": 2026,
            "revenue": 500000, "cost_of_goods_sold": 300000,
            "operating_expenses": 100000, "depreciation": 20000,
            "interest_expense": 10000, "tax_rate": 0.25,
            "shares_outstanding": 10000, "diluted_shares": 12000
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["gross_profit"] == 200000
        assert data["ebitda"] == 100000
        assert data["ebit"] == 80000
        assert data["eps"] is not None
        assert data["diluted_eps"] is not None
        assert data["net_income"] > 0


class TestVATReportingService:
    def setup_method(self):
        self.client = TestClient(load_service_app("vat-reporting-service"))
    
    def test_vat_return(self):
        resp = self.client.post("/prepare-return", json={
            "company_id": "comp-1", "tax_period": "2026-Q1",
            "output_transactions": [
                {"description": "Sales", "amount": 10000, "vat_rate": 0.15, "transaction_type": "standard"},
                {"description": "Export", "amount": 5000, "vat_rate": 0.0, "transaction_type": "zero_rated"}
            ],
            "input_transactions": [
                {"description": "Supplies", "amount": 4000, "vat_rate": 0.15, "transaction_type": "standard"}
            ]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_output_vat"] == 1500
        assert data["total_input_vat"] == 600
        assert data["vat_due_to_authority"] == 900
        assert data["standard_rated_sales"] == 10000
        assert data["zero_rated_sales"] == 5000


class TestSuspenseErrorService:
    def setup_method(self):
        self.client = TestClient(load_service_app("suspense-error-service"))
    
    def test_suspense_analysis(self):
        resp = self.client.post("/analyze", json={
            "company_id": "comp-1", "period": "2026",
            "suspense_balance": 500,
            "errors": [
                {"error_type": "omission", "description": "Missing entry", "amount": 200,
                 "account_affected": "Sales", "correct_debit": "Cash", "correct_credit": "Sales"},
                {"error_type": "commission", "description": "Wrong account", "amount": 100,
                 "account_affected": "Equipment", "correct_debit": "Machinery", "correct_credit": "Cash"}
            ]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_errors_found"] == 2
        assert len(data["corrections"]) == 2


class TestLabourVarianceServices:
    def setup_method(self):
        self.client_cost = TestClient(load_service_app("labour-cost-variance-service"))
        self.client_eff = TestClient(load_service_app("labour-efficiency-variance-service"))
    
    def test_labour_cost_variance(self):
        resp = self.client_cost.post("/analyze", json={
            "company_id": "comp-1", "period": "2026",
            "departments": [
                {"department": "Assembly", "standard_rate": 20, "actual_rate": 22,
                 "standard_hours": 1000, "actual_hours": 950, "standard_output": 1000, "actual_output": 950}
            ]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_rate_variance"] == 1900  # (22-20)*950
        assert data["total_efficiency_variance"] == 0  # std_hours_for_actual = 950, actual = 950
    
    def test_labour_efficiency(self):
        resp = self.client_eff.post("/analyze", json={
            "company_id": "comp-1", "period": "2026",
            "departments": [
                {"department": "Assembly", "standard_rate": 20,
                 "standard_hours_per_unit": 1.0, "actual_hours": 950, "actual_output": 1000,
                 "idle_time_hours": 50}
            ]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_idle_time_variance"] == 1000  # 50 * 20


class TestPartnershipSaleService:
    def setup_method(self):
        self.client = TestClient(load_service_app("partnership-sale-service"))
    
    def test_retirement_calculation(self):
        resp = self.client.post("/retirement", json={
            "company_id": "comp-1", "outgoing_partner_id": "p1",
            "outgoing_partner_name": "Alice",
            "capital_balance": 100000, "share_of_goodwill": 20000,
            "share_of_reserves": 5000, "loan_to_partner": 5000,
            "agreed_payment": 140000,
            "remaining_partners": [
                {"partner_id": "p2", "name": "Bob", "share_pct": 60},
                {"partner_id": "p3", "name": "Carol", "share_pct": 40}
            ]
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_payable"] == 130000
        assert data["agreed_payment"] == 140000
        assert data["gain_or_loss_on_retirement"] == 10000
        assert len(data["remaining_partner_adjustments"]) == 2
