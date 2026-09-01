"""
Integration tests for Treasury Management, Compliance, and Analytics services.
"""
import pytest
from tests.conftest import load_service
from fastapi.testclient import TestClient
from datetime import datetime, timezone


@pytest.fixture
def treasury_client():
    app = load_service("treasury-management-service").main.app
    return TestClient(app)

@pytest.fixture
def compliance_client():
    app = load_service("treasury-compliance-service").main.app
    return TestClient(app)

@pytest.fixture
def analytics_client():
    app = load_service("treasury-analytics-service").main.app
    return TestClient(app)


class TestTreasuryManagement:
    def test_health(self, treasury_client):
        resp = treasury_client.get("/")
        assert resp.status_code == 200
        assert resp.json()["service"] == "treasury-management-service"

    def test_record_cashflow(self, treasury_client):
        resp = treasury_client.post("/cashflows", json={
            "company_id": "comp-1", "flow_type": "inflow", "amount": 50000,
            "currency": "USD", "description": "Customer payment"
        })
        assert resp.status_code == 200
        assert resp.json()["status"] == "recorded"

    def test_get_cashflows(self, treasury_client):
        treasury_client.post("/cashflows", json={
            "company_id": "comp-2", "flow_type": "inflow", "amount": 10000
        })
        resp = treasury_client.get("/cashflows/comp-2")
        assert resp.status_code == 200
        assert resp.json()["total"] >= 1

    def test_cash_position(self, treasury_client):
        treasury_client.post("/cashflows", json={"company_id": "comp-pos", "flow_type": "inflow", "amount": 50000})
        treasury_client.post("/cashflows", json={"company_id": "comp-pos", "flow_type": "outflow", "amount": 20000})
        resp = treasury_client.get("/position/comp-pos")
        assert resp.status_code == 200
        data = resp.json()
        assert data["total_cash"] == 30000  # 50000 - 20000
        assert data["available_cash"] == 30000

    def test_forecast(self, treasury_client):
        treasury_client.post("/cashflows", json={"company_id": "comp-fc", "flow_type": "inflow", "amount": 30000})
        resp = treasury_client.post("/forecast/comp-fc?days=30")
        assert resp.status_code == 200
        data = resp.json()
        assert data["projected_inflows"] >= 0
        assert data["projected_outflows"] >= 0
        assert "assumptions" in data

    def test_investment_options(self, treasury_client):
        resp = treasury_client.get("/investment-options")
        assert resp.status_code == 200
        assert len(resp.json()["options"]) >= 3


class TestTreasuryCompliance:
    def test_health(self, compliance_client):
        resp = compliance_client.get("/")
        assert resp.status_code == 200

    def test_get_checks(self, compliance_client):
        resp = compliance_client.get("/checks/comp-1")
        assert resp.status_code == 200
        data = resp.json()
        assert data["total"] >= 6  # 6 default checks
        assert "compliance_rate" in data

    def test_update_check_status(self, compliance_client):
        checks = compliance_client.get("/checks/comp-1").json()
        check_id = checks["checks"][0]["id"]
        resp = compliance_client.put(f"/checks/{check_id}/status?status=non_compliant&remediation=Fix required")
        assert resp.status_code == 200
        assert resp.json()["status"] == "non_compliant"

    def test_compliance_report(self, compliance_client):
        resp = compliance_client.get("/report/comp-1")
        assert resp.status_code == 200
        assert "compliance_rate" in resp.json()


class TestTreasuryAnalytics:
    def test_health(self, analytics_client):
        resp = analytics_client.get("/")
        assert resp.status_code == 200

    def test_analyze(self, analytics_client):
        resp = analytics_client.post("/analyze", json={
            "company_id": "comp-1",
            "total_cash": 500000,
            "monthly_inflow": 200000,
            "monthly_outflow": 150000,
            "short_term_debt": 30000,
            "total_debt": 100000,
            "investments": 200000,
            "fx_exposure": 50000
        })
        assert resp.status_code == 200
        data = resp.json()
        assert len(data["kpis"]) >= 5
        assert data["cash_adequacy_days"] > 0
        assert data["debt_service_ratio"] >= 0

    def test_kpi_lookup(self, analytics_client):
        analytics_client.post("/analyze", json={
            "company_id": "comp-kpi", "total_cash": 100000, "monthly_inflow": 50000,
            "monthly_outflow": 30000
        })
        resp = analytics_client.get("/kpi/comp-kpi")
        assert resp.status_code == 200
