"""
Integration tests for Risk Assessment, Mitigation, Reporting, and Investigation services.
"""
import pytest
from tests.conftest import load_service
from fastapi.testclient import TestClient


@pytest.fixture
def risk_client():
    app = load_service("risk-assessment-service").main.app
    return TestClient(app)

@pytest.fixture
def mitigation_client():
    app = load_service("risk-mitigation-service").main.app
    return TestClient(app)

@pytest.fixture
def reporting_client():
    app = load_service("risk-reporting-service").main.app
    return TestClient(app)

@pytest.fixture
def investigation_client():
    app = load_service("investigation-service").main.app
    return TestClient(app)


class TestRiskAssessment:
    def test_health(self, risk_client):
        resp = risk_client.get("/")
        assert resp.status_code == 200
        assert resp.json()["service"] == "risk-assessment-service"

    def test_create_risk(self, risk_client):
        resp = risk_client.post("/risks", json={
            "company_id": "comp-1", "category": "financial", "name": "Currency Risk",
            "description": "FX exposure risk", "likelihood": 4, "impact": 3
        })
        assert resp.status_code == 200
        data = resp.json()
        assert data["risk_score"] == 12  # 4 * 3
        assert data["level"] == "high"

    def test_create_critical_risk(self, risk_client):
        resp = risk_client.post("/risks", json={
            "company_id": "comp-2", "category": "operational", "name": "System Failure",
            "likelihood": 5, "impact": 5
        })
        assert resp.status_code == 200
        assert resp.json()["risk_score"] == 25
        assert resp.json()["level"] == "critical"

    def test_get_risks_filter(self, risk_client):
        risk_client.post("/risks", json={"company_id": "comp-filter", "category": "financial", "name": "R1", "likelihood": 2, "impact": 2})
        risk_client.post("/risks", json={"company_id": "comp-filter", "category": "cyber", "name": "R2", "likelihood": 4, "impact": 4})
        resp = risk_client.get("/risks/comp-filter?category=cyber")
        assert resp.status_code == 200
        data = resp.json()
        assert data["total"] == 1
        assert data["risks"][0]["category"] == "cyber"

    def test_update_risk(self, risk_client):
        create = risk_client.post("/risks", json={"company_id": "comp-up", "category": "compliance", "name": "R3", "likelihood": 2, "impact": 2})
        risk_id = create.json()["id"]
        resp = risk_client.put(f"/risks/{risk_id}?likelihood=5&impact=5&mitigation=Fixed&status=mitigating")
        assert resp.status_code == 200
        data = resp.json()
        assert data["risk_score"] == 25
        assert data["level"] == "critical"

    def test_risk_dashboard(self, risk_client):
        risk_client.post("/risks", json={"company_id": "comp-dash", "category": "financial", "name": "R1", "likelihood": 3, "impact": 4})
        risk_client.post("/risks", json={"company_id": "comp-dash", "category": "cyber", "name": "R2", "likelihood": 5, "impact": 5})
        resp = risk_client.get("/dashboard/comp-dash")
        assert resp.status_code == 200
        data = resp.json()
        assert data["total_risks"] == 2
        assert len(data["top_risks"]) == 2
        assert "by_level" in data

    def test_close_risk(self, risk_client):
        create = risk_client.post("/risks", json={"company_id": "comp-close", "category": "market", "name": "R", "likelihood": 1, "impact": 1})
        risk_id = create.json()["id"]
        resp = risk_client.delete(f"/risks/{risk_id}")
        assert resp.status_code == 200
        assert resp.json()["status"] == "closed"


class TestRiskMitigation:
    def test_health(self, mitigation_client):
        resp = mitigation_client.get("/")
        assert resp.status_code == 200

    def test_create_and_mitigate(self, mitigation_client):
        create = mitigation_client.post("/risks", json={
            "company_id": "comp-mit", "category": "operational", "name": "Process Risk",
            "likelihood": 3, "impact": 4
        })
        assert create.json()["risk_score"] == 12
        risk_id = create.json()["id"]
        update = mitigation_client.put(f"/risks/{risk_id}?mitigation=Implemented+new+controls&status=mitigating")
        assert update.status_code == 200
        assert update.json()["status"] == "mitigating"


class TestRiskReporting:
    def test_health(self, reporting_client):
        assert reporting_client.get("/").status_code == 200

    def test_report_dashboard(self, reporting_client):
        reporting_client.post("/risks", json={"company_id": "comp-rpt", "category": "compliance", "name": "Reg Risk", "likelihood": 4, "impact": 3})
        resp = reporting_client.get("/dashboard/comp-rpt")
        assert resp.status_code == 200
        assert resp.json()["total_risks"] >= 1


class TestInvestigation:
    def test_health(self, investigation_client):
        assert investigation_client.get("/").status_code == 200

    def test_investigation_workflow(self, investigation_client):
        create = investigation_client.post("/risks", json={
            "company_id": "comp-inv", "category": "financial", "name": "Suspicious Activity",
            "likelihood": 4, "impact": 5
        })
        risk_id = create.json()["id"]
        update = investigation_client.put(f"/risks/{risk_id}?status=assessing")
        assert update.json()["status"] == "assessing"
        close = investigation_client.delete(f"/risks/{risk_id}")
        assert close.json()["status"] == "closed"
