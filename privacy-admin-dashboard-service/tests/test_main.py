import pytest
from fastapi.testclient import TestClient
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))
from main import app

client = TestClient(app)

def test_health_check():
    response = client.get("/health")
    assert response.status_code == 200
    assert response.json()["status"] == "healthy"

def test_operational_metrics_with_valid_admin_token():
    response = client.get("/metrics/operational", headers={"authorization": "Bearer admin_secret_token"})
    assert response.status_code == 200
    data = response.json()
    assert "registered_users" in data
    assert "active_users_monthly" in data
    assert "feature_usage_counts" in data

def test_metrics_do_not_contain_personal_finance_data():
    response = client.get("/metrics/operational", headers={"authorization": "Bearer admin_secret_token"})
    assert response.status_code == 200
    data = response.json()
    # Ensure no personal financial data fields exist
    forbidden_fields = ["income", "expenses", "revenue", "salary", "bank_balance"]
    for field in forbidden_fields:
        assert field not in data, f"Field '{field}' should not be in admin metrics"

def test_metrics_forbidden_without_auth():
    response = client.get("/metrics/operational")
    assert response.status_code == 403

def test_metrics_forbidden_with_wrong_token():
    response = client.get("/metrics/operational", headers={"authorization": "Bearer wrong_token"})
    assert response.status_code == 403
