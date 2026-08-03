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

def test_list_all_plans():
    response = client.get("/plans")
    assert response.status_code == 200
    plans = response.json()
    assert len(plans) == 7
    plan_ids = [p["plan_id"] for p in plans]
    for expected in ["free", "family", "basic", "professional", "business", "enterprise", "government"]:
        assert expected in plan_ids

def test_get_free_plan():
    response = client.get("/plans/free")
    assert response.status_code == 200
    data = response.json()
    assert data["plan_id"] == "free"
    assert data["price_monthly"] == 0.0

def test_get_family_plan():
    response = client.get("/plans/family")
    assert response.status_code == 200
    data = response.json()
    assert data["plan_id"] == "family"
    assert "Expense splitting" in data["features"]
    assert "Role-based permissions" in data["features"]

def test_get_enterprise_plan():
    response = client.get("/plans/enterprise")
    assert response.status_code == 200
    data = response.json()
    assert "Single Sign-On (SSO)" in data["features"]

def test_get_government_plan():
    response = client.get("/plans/government")
    assert response.status_code == 200
    data = response.json()
    assert data["plan_id"] == "government"
    assert "Compliance reporting" in data["features"]

def test_plan_not_found():
    response = client.get("/plans/nonexistent")
    assert response.status_code == 404
