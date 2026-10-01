"""
Integration tests for the Fraud Detection Service.
Tests all fraud rules, risk assessment, and alert management.
"""

from datetime import datetime, timedelta, timezone

import pytest
from fastapi.testclient import TestClient


@pytest.fixture
def client():
    import importlib.util
    import os

    from tests.conftest import load_service

    svc = load_service("fraud-detection-service")
    app = svc.main.app
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    fake_path = os.path.join(repo_root, "fraud-detection-service", "fake_neo4j.py")
    import sys

    spec = importlib.util.spec_from_file_location("fd_root_fake", fake_path)
    fake_mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fake_mod)
    session = fake_mod.FakeSession()
    # conftest replaces the parent alias with a bare module (no __path__), so
    # load the lazily-imported database module by file path and patch it
    db_spec = importlib.util.spec_from_file_location(
        "fraud_detection_service.database", os.path.join(os.path.dirname(fake_path), "database.py")
    )
    db_mod = importlib.util.module_from_spec(db_spec)
    sys.modules["fraud_detection_service.database"] = db_mod
    db_spec.loader.exec_module(db_mod)
    db_mod.Neo4jConnector.get_driver = classmethod(lambda cls: fake_mod.FakeDriver(session))
    c = TestClient(app)
    c.headers.update({"X-User-Id": "fraud-root-user"})
    return c


@pytest.fixture
def sample_transactions():
    # Use noon UTC to avoid off-hours alerts
    noon = datetime(2026, 1, 15, 12, 0, 0, tzinfo=timezone.utc)
    return [
        {
            "company_id": "comp-1",
            "account_id": "acc-1",
            "amount": 1000,
            "currency": "USD",
            "timestamp": noon.isoformat(),
            "description": "Office supplies",
            "merchant": "Staples",
        },
        {
            "company_id": "comp-1",
            "account_id": "acc-1",
            "amount": 500,
            "currency": "USD",
            "timestamp": noon.isoformat(),
            "description": "Lunch",
            "merchant": "Restaurant",
        },
    ]


class TestHealth:
    def test_health_check(self, client):
        resp = client.get("/")
        assert resp.status_code == 200
        data = resp.json()
        assert data["status"] == "healthy"
        assert data["service"] == "fraud-detection-service"

    def test_health_alias(self, client):
        resp = client.get("/health")
        assert resp.status_code == 200
        assert resp.json()["status"] == "healthy"


class TestFraudDetection:
    def test_detect_no_fraud(self, client, sample_transactions):
        resp = client.post("/detect", json={"company_id": "comp-1", "transactions": sample_transactions})
        assert resp.status_code == 200
        data = resp.json()
        assert data["transactions_analyzed"] == 2
        assert data["fraudulent_detected"] == 0
        assert len(data["alerts"]) == 0
        assert data["risk_assessment"]["overall_risk_level"] == "minimal"

    def test_detect_large_transaction(self, client):
        tx = [
            {
                "company_id": "comp-1",
                "account_id": "acc-1",
                "amount": 60000,
                "timestamp": datetime(2026, 1, 15, 12, 0, 0, tzinfo=timezone.utc).isoformat(),
                "description": "Large transfer",
                "merchant": "Bank",
            }
        ]
        resp = client.post("/detect", json={"company_id": "comp-1", "transactions": tx})
        assert resp.status_code == 200
        data = resp.json()
        assert data["fraudulent_detected"] == 1
        assert len(data["alerts"]) >= 1
        assert data["alerts"][0]["rule_name"] == "Large Transaction Alert"
        assert data["alerts"][0]["severity"] == "high"

    def test_detect_duplicate_transaction(self, client):
        ts = datetime(2026, 1, 15, 12, 0, 0, tzinfo=timezone.utc).isoformat()
        tx = [
            {
                "company_id": "comp-1",
                "account_id": "acc-1",
                "amount": 5000,
                "timestamp": ts,
                "description": "Payment",
                "merchant": "Supplier A",
            },
            {
                "company_id": "comp-1",
                "account_id": "acc-1",
                "amount": 5000,
                "timestamp": ts,
                "description": "Payment",
                "merchant": "Supplier A",
            },
        ]
        resp = client.post("/detect", json={"company_id": "comp-1", "transactions": tx})
        assert resp.status_code == 200
        data = resp.json()
        # Duplicate should trigger
        dup_alerts = [a for a in data["alerts"] if "Duplicate" in a["rule_name"]]
        assert len(dup_alerts) >= 1

    def test_detect_round_amount(self, client):
        tx = [
            {
                "company_id": "comp-1",
                "account_id": "acc-1",
                "amount": 10000,
                "timestamp": datetime(2026, 1, 15, 12, 0, 0, tzinfo=timezone.utc).isoformat(),
                "description": "Payment",
                "merchant": "X",
            }
        ]
        resp = client.post("/detect", json={"company_id": "comp-1", "transactions": tx})
        data = resp.json()
        round_alerts = [a for a in data["alerts"] if "Round" in a["rule_name"]]
        assert len(round_alerts) >= 1

    def test_detect_empty_transactions(self, client):
        resp = client.post("/detect", json={"company_id": "comp-1", "transactions": []})
        assert resp.status_code == 400


class TestAlertManagement:
    def test_get_alerts(self, client):
        # First trigger an alert
        tx = [
            {
                "company_id": "comp-2",
                "account_id": "acc-1",
                "amount": 80000,
                "timestamp": datetime(2026, 1, 15, 12, 0, 0, tzinfo=timezone.utc).isoformat(),
                "description": "Big",
                "merchant": "X",
            }
        ]
        client.post("/detect", json={"company_id": "comp-2", "transactions": tx})
        # Get alerts
        resp = client.get("/alerts/comp-2")
        assert resp.status_code == 200
        data = resp.json()
        assert data["total"] >= 1

    def test_update_alert_status(self, client):
        tx = [
            {
                "company_id": "comp-3",
                "account_id": "acc-1",
                "amount": 70000,
                "timestamp": datetime(2026, 1, 15, 12, 0, 0, tzinfo=timezone.utc).isoformat(),
                "description": "Big",
                "merchant": "X",
            }
        ]
        client.post("/detect", json={"company_id": "comp-3", "transactions": tx})
        alerts = client.get("/alerts/comp-3").json()
        alert_id = alerts["alerts"][0]["id"]
        resp = client.put(f"/alerts/{alert_id}/status?new_status=confirmed")
        assert resp.status_code == 200
        assert resp.json()["status"] == "confirmed"


class TestRules:
    def test_get_default_rules(self, client):
        resp = client.get("/rules/comp-1")
        assert resp.status_code == 200
        data = resp.json()
        assert len(data["rules"]) >= 6  # 6 default rules

    def test_add_custom_rule(self, client):
        rule = {
            "name": "Custom Rule",
            "description": "Test custom rule",
            "rule_type": "amount_threshold",
            "parameters": {"threshold": 1000},
            "severity": "low",
            "enabled": True,
        }
        resp = client.post("/rules/comp-custom", json=rule)
        assert resp.status_code == 200
        assert resp.json()["status"] == "added"

    def test_toggle_rule(self, client):
        rules = client.get("/rules/comp-1").json()
        rule_id = rules["rules"][0]["id"]
        resp = client.put(f"/rules/{rule_id}?enabled=false")
        assert resp.status_code == 200
        assert resp.json()["enabled"] == False


class TestRiskAssessment:
    def test_risk_no_alerts(self, client):
        resp = client.get("/risk/comp-empty")
        assert resp.status_code == 200
        data = resp.json()
        assert data["overall_risk_level"] == "minimal"
        assert data["risk_score"] == 0

    def test_risk_with_alerts(self, client):
        tx = [
            {
                "company_id": "comp-risk",
                "account_id": "acc-1",
                "amount": 100000,
                "timestamp": datetime(2026, 1, 15, 12, 0, 0, tzinfo=timezone.utc).isoformat(),
                "description": "Huge",
                "merchant": "X",
            }
        ]
        client.post("/detect", json={"company_id": "comp-risk", "transactions": tx})
        resp = client.get("/risk/comp-risk")
        data = resp.json()
        assert data["risk_score"] > 0
        assert data["overall_risk_level"] in ("high", "extreme", "moderate", "low")
