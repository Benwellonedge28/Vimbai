"""
Integration tests for Audit services (Forensic, IT, Operational, Tax)
and Costing services (Process, Product, Job).
"""

from datetime import datetime, timezone

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service


@pytest.fixture
def forensic_client():
    app = load_service("forensic-accounting-service").main.app
    return TestClient(app)


@pytest.fixture
def it_audit_client():
    app = load_service("it-audit-service").main.app
    return TestClient(app)


@pytest.fixture
def operational_audit_client():
    app = load_service("operational-audit-service").main.app
    return TestClient(app)


@pytest.fixture
def tax_audit_client():
    app = load_service("tax-audit-service").main.app
    return TestClient(app)


@pytest.fixture
def process_costing_client():
    app = load_service("process-costing-service").main.app
    return TestClient(app)


@pytest.fixture
def product_costing_client():
    app = load_service("product-costing-service").main.app
    return TestClient(app)


@pytest.fixture
def job_costing_client():
    app = load_service("job-costing-service").main.app
    return TestClient(app)


class TestForensicAccounting:
    def test_health(self, forensic_client):
        assert forensic_client.get("/").status_code == 200

    def test_engagement_lifecycle(self, forensic_client):
        # Create
        create = forensic_client.post(
            "/engagements",
            json={
                "company_id": "comp-1",
                "audit_type": "forensic",
                "title": "Fraud Investigation Q1",
                "scope": "Financial records",
                "objectives": ["Identify fraud", "Quantify loss"],
            },
        )
        assert create.status_code == 200
        eng_id = create.json()["id"]

        # Add findings
        finding = forensic_client.post(
            f"/engagements/{eng_id}/findings",
            json={
                "title": "Misappropriation",
                "description": "Funds diverted",
                "severity": "critical",
                "recommendation": "Terminate employee, recover funds",
            },
        )
        assert finding.status_code == 200

        # Remediate finding
        finding_id = finding.json()["finding_id"]
        rem = forensic_client.put(f"/findings/{finding_id}/remediate?remediation_note=Recovered+80%%20of+funds")
        assert rem.status_code == 200

        # Complete engagement
        complete = forensic_client.put(f"/engagements/{eng_id}/status?status=completed&summary=Investigation+complete")
        assert complete.json()["status"] == "completed"

        # Get report
        report = forensic_client.get(f"/report/{eng_id}")
        assert report.status_code == 200
        assert report.json()["findings_summary"]["critical"] == 1


class TestITAudit:
    def test_health(self, it_audit_client):
        assert it_audit_client.get("/").status_code == 200

    def test_audit_with_findings(self, it_audit_client):
        create = it_audit_client.post(
            "/engagements",
            json={
                "company_id": "comp-2",
                "audit_type": "it",
                "title": "IT Security Audit",
                "scope": "Access controls and data security",
            },
        )
        eng_id = create.json()["id"]
        it_audit_client.post(
            f"/engagements/{eng_id}/findings",
            json={"title": "Weak Passwords", "severity": "high", "description": "Many users have weak passwords"},
        )
        report = it_audit_client.get(f"/report/{eng_id}")
        assert report.json()["findings_summary"]["high"] == 1


class TestOperationalAudit:
    def test_health(self, operational_audit_client):
        assert operational_audit_client.get("/").status_code == 200

    def test_create_engagement(self, operational_audit_client):
        resp = operational_audit_client.post(
            "/engagements",
            json={
                "company_id": "comp-3",
                "audit_type": "operational",
                "title": "Process Audit",
                "scope": "Manufacturing processes",
            },
        )
        assert resp.status_code == 200
        assert resp.json()["status"] == "planned"


class TestTaxAudit:
    def test_health(self, tax_audit_client):
        assert tax_audit_client.get("/").status_code == 200

    def test_full_audit_cycle(self, tax_audit_client):
        create = tax_audit_client.post(
            "/engagements", json={"company_id": "comp-4", "audit_type": "tax", "title": "Tax Compliance Audit 2026"}
        )
        eng_id = create.json()["id"]
        # Add findings
        tax_audit_client.post(
            f"/engagements/{eng_id}/findings",
            json={"title": "Under-reported income", "severity": "high", "description": "Income not fully reported"},
        )
        # Complete
        tax_audit_client.put(f"/engagements/{eng_id}/status?status=completed")
        # Verify
        report = tax_audit_client.get(f"/report/{eng_id}")
        assert report.json()["findings_summary"]["total"] >= 1


class TestProcessCosting:
    def test_health(self, process_costing_client):
        assert process_costing_client.get("/").status_code == 200

    def test_calculate_cost(self, process_costing_client):
        resp = process_costing_client.post(
            "/calculate",
            json={
                "company_id": "comp-1",
                "product_or_process": "Assembly Line A",
                "period": "2026-01",
                "quantity": 1000,
                "components": [
                    {"name": "Raw materials", "amount": 50000, "cost_type": "direct_materials"},
                    {"name": "Labor", "amount": 30000, "cost_type": "direct_labor"},
                    {"name": "Factory overhead", "amount": 20000, "cost_type": "overhead"},
                ],
            },
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["total_cost"] == 100000
        assert data["unit_cost"] == 100.0

    def test_cost_breakdown(self, process_costing_client):
        calc = process_costing_client.post(
            "/calculate",
            json={
                "company_id": "comp-bd",
                "product_or_process": "Product X",
                "quantity": 100,
                "components": [
                    {"name": "Materials", "amount": 40000, "cost_type": "direct_materials"},
                    {"name": "Labor", "amount": 20000, "cost_type": "direct_labor"},
                ],
            },
        )
        calc_id = calc.json()["id"]
        resp = process_costing_client.get(f"/breakdown/comp-bd/{calc_id}")
        assert resp.status_code == 200
        assert "direct_materials" in resp.json()["breakdown"]
        assert "direct_labor" in resp.json()["breakdown"]


class TestProductCosting:
    def test_health(self, product_costing_client):
        assert product_costing_client.get("/").status_code == 200

    def test_calculate(self, product_costing_client):
        resp = product_costing_client.post(
            "/calculate",
            json={
                "company_id": "comp-1",
                "product_or_process": "Widget Pro",
                "quantity": 500,
                "components": [
                    {"name": "Components", "amount": 25000, "cost_type": "direct_materials"},
                    {"name": "Assembly", "amount": 15000, "cost_type": "direct_labor"},
                ],
            },
        )
        assert resp.json()["unit_cost"] == 80.0  # 40000/500


class TestJobCosting:
    def test_health(self, job_costing_client):
        assert job_costing_client.get("/").status_code == 200

    def test_job_lifecycle(self, job_costing_client):
        # Create job
        create = job_costing_client.post(
            "/jobs",
            json={"company_id": "comp-1", "job_name": "Custom Build", "customer": "Client A", "contract_value": 100000},
        )
        job_id = create.json()["id"]

        # Add costs
        job_costing_client.post(
            f"/jobs/{job_id}/costs",
            json={"cost_type": "materials", "amount": 30000, "description": "Building materials"},
        )
        job_costing_client.post(
            f"/jobs/{job_id}/costs", json={"cost_type": "labor", "amount": 20000, "description": "Construction labor"}
        )
        job_costing_client.post(
            f"/jobs/{job_id}/costs",
            json={"cost_type": "overhead", "amount": 10000, "description": "Overhead allocation"},
        )

        # Check profitability
        resp = job_costing_client.get("/jobs/comp-1/profitability")
        assert resp.status_code == 200
        data = resp.json()
        assert data["total_cost"] == 60000
        assert data["total_profit"] == 40000  # 100000 - 60000
