"""
Integration tests for the upgraded partial services.
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


class TestAutomationEngineService:
    def setup_method(self):
        self.client = TestClient(load_app("automation-engine-service"))
    
    def test_health(self):
        assert self.client.get("/health").status_code == 200
    
    def test_create_and_list_rule(self):
        resp = self.client.post("/rules", json={
            "name": "Auto-reconcile", "company_id": "comp-1",
            "trigger": "scheduled", "steps": [
                {"step_id": "s1", "step_name": "Fetch", "action": "GET /transactions", "params": {}}
            ]
        })
        assert resp.status_code == 200
        rule_id = resp.json()["id"]
        assert rule_id
        
        resp = self.client.get("/rules", params={"company_id": "comp-1"})
        assert len(resp.json()) >= 1
    
    def test_execute_rule(self):
        create = self.client.post("/rules", json={
            "name": "Test Rule", "company_id": "comp-1",
            "trigger": "manual", "steps": [
                {"step_id": "s1", "step_name": "Step 1", "action": "do_something"}
            ]
        })
        rule_id = create.json()["id"]
        resp = self.client.post(f"/execute/{rule_id}")
        assert resp.status_code == 200
        assert resp.json()["status"] in ("running", "completed")


class TestSupplyChainService:
    def setup_method(self):
        self.client = TestClient(load_app("supply-chain-service"))
    
    def test_supplier_crud(self):
        resp = self.client.post("/suppliers", json={"name": "Acme Corp", "lead_time_days": 7, "rating": 4.5})
        assert resp.status_code == 200
        assert resp.json()["name"] == "Acme Corp"
        
        resp = self.client.get("/suppliers")
        assert len(resp.json()) >= 1
    
    def test_inventory_and_low_stock(self):
        self.client.post("/inventory", json={
            "sku": "WIDGET-001", "name": "Widget", "company_id": "comp-1",
            "quantity": 5, "reorder_point": 10, "reorder_qty": 50
        })
        resp = self.client.get("/inventory/low-stock", params={"company_id": "comp-1"})
        data = resp.json()
        assert len(data) >= 1
        assert data[0]["urgency"] in ("critical", "warning")
    
    def test_demand_forecast(self):
        resp = self.client.post("/forecast", json={
            "sku": "PROD-001", "company_id": "comp-1",
            "historical_data": [100, 110, 105, 120, 115], "forecast_periods": 3
        })
        data = resp.json()
        assert resp.status_code == 200
        assert len(data["forecast"]) == 3
        assert data["method"] == "moving_average_with_trend"
    
    def test_purchase_order_flow(self):
        po = self.client.post("/purchase-orders", json={
            "company_id": "comp-1", "supplier_id": "sup-1",
            "item_sku": "WIDGET-001", "quantity": 100, "unit_cost": 5.0
        })
        po_id = po.json()["id"]
        receive = self.client.post(f"/purchase-orders/{po_id}/receive", params={"company_id": "comp-1"})
        assert receive.status_code == 200
        assert receive.json()["status"] == "received"


class TestInventoryValuationService:
    def setup_method(self):
        self.client = TestClient(load_app("inventory-valuation-service"))
    
    def test_fifo(self):
        resp = self.client.post("/calculate", json={
            "company_id": "comp-1", "method": "fifo",
            "purchases": [
                {"date": "2026-01-01", "quantity": 100, "unit_cost": 10},
                {"date": "2026-02-01", "quantity": 50, "unit_cost": 12}
            ],
            "sales": [{"date": "2026-03-01", "quantity": 80, "unit_price": 15}],
            "opening_inventory": 0, "opening_qty": 0
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["cost_of_goods_sold"] == 800  # 80 * 10 (FIFO)
        assert data["ending_qty"] == 70  # 150 - 80
    
    def test_weighted_average(self):
        resp = self.client.post("/calculate", json={
            "company_id": "comp-1", "method": "weighted_average",
            "purchases": [
                {"date": "2026-01-01", "quantity": 100, "unit_cost": 10},
                {"date": "2026-02-01", "quantity": 50, "unit_cost": 12}
            ],
            "sales": [{"date": "2026-03-01", "quantity": 80, "unit_price": 15}],
            "opening_inventory": 0, "opening_qty": 0
        })
        data = resp.json()
        avg_cost = (1000 + 600) / 150
        assert data["cost_of_goods_sold"] == round(80 * avg_cost, 2)


class TestDashboardService:
    def setup_method(self):
        self.client = TestClient(load_app("dashboard-service"))
    
    def test_executive_dashboard(self):
        resp = self.client.post("/load", json={
            "company_id": "comp-1", "dashboard_type": "executive", "refresh_interval": 30
        })
        data = resp.json()
        assert resp.status_code == 200
        assert "executive" in data["dashboard_type"]
        assert len(data["widgets"]) >= 4
        widget_types = [w["type"] for w in data["widgets"]]
        assert "kpi_card" in widget_types
        assert "chart" in widget_types
    
    def test_financial_dashboard(self):
        resp = self.client.post("/load", json={
            "company_id": "comp-1", "dashboard_type": "financial", "refresh_interval": 60
        })
        data = resp.json()
        assert any(w["title"] == "Current Ratio" for w in data["widgets"])


class TestEnterpriseSSOService:
    def setup_method(self):
        self.client = TestClient(load_app("enterprise-sso-service"))
    
    def test_oidc_auth(self):
        # Create a fake JWT-like token
        import base64, json
        header = base64.urlsafe_b64encode(json.dumps({"alg": "RS256"}).encode()).rstrip(b"=").decode()
        payload = base64.urlsafe_b64encode(json.dumps({"sub": "user123", "exp": 99999999999}).encode()).rstrip(b"=").decode()
        token = f"{header}.{payload}.signature"
        
        resp = self.client.post("/auth/sso", json={
            "organization_id": "org-1", "idp_token": token, "provider": "oidc"
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["user_id"] == "user123"
        assert data["vimbai_access_token"]
    
    def test_invalid_token(self):
        resp = self.client.post("/auth/sso", json={
            "organization_id": "org-1", "idp_token": "short", "provider": "oidc"
        })
        assert resp.status_code == 401
    
    def test_session_validation(self):
        import base64, json
        header = base64.urlsafe_b64encode(json.dumps({"alg": "RS256"}).encode()).rstrip(b"=").decode()
        payload = base64.urlsafe_b64encode(json.dumps({"sub": "user456", "exp": 99999999999}).encode()).rstrip(b"=").decode()
        token = f"{header}.{payload}.sig"
        
        auth = self.client.post("/auth/sso", json={"organization_id": "org-1", "idp_token": token, "provider": "oidc"})
        vimbai_token = auth.json()["vimbai_access_token"]
        
        resp = self.client.get(f"/session/{vimbai_token}")
        assert resp.status_code == 200
        assert resp.json()["valid"] is True


class TestAmortizationService:
    def setup_method(self):
        self.client = TestClient(load_app("amortization-service"))
    
    def test_straight_line(self):
        resp = self.client.post("/calculate", json={
            "company_id": "comp-1",
            "intangibles": [
                {"intangible_id": "pat1", "type": "patent", "cost": 100000,
                 "useful_life_years": 10, "residual_value": 0, "method": "straight_line"}
            ],
            "period_start": "2026-01-01", "period_end": "2026-12-31"
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_amortization"] == 10000
        assert data["schedules"][0]["annual_amortization"] == 10000
        assert data["schedules"][0]["monthly_amortization"] == round(10000/12, 2)


class TestForeignExchangeService:
    def setup_method(self):
        self.client = TestClient(load_app("foreign-exchange-service"))
    
    def test_conversion(self):
        resp = self.client.post("/convert", json={
            "from_currency": "USD", "to_currency": "EUR", "amount": 100
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["rate"] > 0
        assert data["converted_amount"] > 0
    
    def test_hedge_calculation(self):
        resp = self.client.post("/hedge", json={
            "company_id": "comp-1", "currency": "EUR",
            "exposure_amount": 100000, "hedge_ratio": 0.8, "forward_rate": 0.95
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["hedged_amount"] == 80000
        assert data["unhedged_amount"] == 20000


class TestMFAService:
    def setup_method(self):
        self.client = TestClient(load_app("mfa-auth-service"))
    
    def test_setup_and_verify(self):
        setup = self.client.post("/setup", json={"user_id": "user-1", "method": "totp"})
        data = setup.json()
        assert setup.status_code == 200
        assert data["secret"]
        assert len(data["backup_codes"]) == 8
        
        verify = self.client.post("/verify", json={"user_id": "user-1", "code": "123456"})
        assert verify.status_code == 200
    
    def test_challenge_flow(self):
        challenge = self.client.post("/challenge", params={"user_id": "user-2", "method": "sms"})
        data = challenge.json()
        assert "challenge_id" in data
        # Can't verify without knowing the code, but structure should work


class TestDebtManagementService:
    def setup_method(self):
        self.client = TestClient(load_app("debt-management-service"))
    
    def test_loan_and_schedule(self):
        loan = self.client.post("/loans", json={
            "company_id": "comp-1", "loan_name": "Bank Loan", "lender": "Stanbic",
            "principal": 100000, "interest_rate": 0.10, "term_months": 36,
            "disbursement_date": "2026-01-01"
        })
        assert loan.status_code == 200
        loan_id = loan.json()["id"]
        
        schedule = self.client.post(f"/loans/{loan_id}/schedule", params={"company_id": "comp-1"})
        data = schedule.json()
        assert len(data) == 36
        # First payment should have highest interest, lowest principal
        assert data[0]["interest_component"] > data[-1]["interest_component"]
    
    def test_debt_summary(self):
        self.client.post("/loans", json={
            "company_id": "comp-2", "loan_name": "Loan 1", "lender": "Bank",
            "principal": 50000, "interest_rate": 0.08, "term_months": 24,
            "disbursement_date": "2026-01-01"
        })
        resp = self.client.get("/summary", params={"company_id": "comp-2", "equity": 200000})
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_debt"] > 0
        assert data["debt_to_equity"] > 0


class TestInsuranceClaimsService:
    def setup_method(self):
        self.client = TestClient(load_app("insurance-claims-service"))
    
    def test_file_and_process_claim(self):
        claim = self.client.post("/file", json={
            "company_id": "comp-1", "policy_number": "POL-001",
            "claim_type": "property", "incident_date": "2026-06-15",
            "claim_amount": 50000, "deductible": 5000, "coverage_limit": 100000
        })
        claim_id = claim.json()["id"]
        assert claim.status_code == 200
        
        result = self.client.post(f"/claims/{claim_id}/process", params={"company_id": "comp-1"})
        data = result.json()
        assert result.status_code == 200
        assert data["covered_amount"] == 45000  # 50000 - 5000 deductible
        assert data["status"] == "approved"


class TestScenarioAnalysisService:
    def setup_method(self):
        self.client = TestClient(load_app("scenario-analysis-service"))
    
    def test_scenario_analysis(self):
        resp = self.client.post("/analyze", json={
            "company_id": "comp-1", "base_revenue": 1000000, "base_cost": 700000,
            "base_interest": 20000, "base_depreciation": 50000,
            "best_case": {"revenue_growth": 0.2, "cost_growth": 0.03, "interest_rate": 0.05, "tax_rate": 0.25, "description": "Optimistic"},
            "base_case": {"revenue_growth": 0.1, "cost_growth": 0.05, "interest_rate": 0.05, "tax_rate": 0.25, "description": "Expected"},
            "worst_case": {"revenue_growth": -0.1, "cost_growth": 0.08, "interest_rate": 0.07, "tax_rate": 0.25, "description": "Pessimistic"}
        })
        data = resp.json()
        assert resp.status_code == 200
        assert data["best_case"]["net_income"] > data["worst_case"]["net_income"]
        assert len(data["recommendation"]) > 0
