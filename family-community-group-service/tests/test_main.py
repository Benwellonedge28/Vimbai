import pytest
from fastapi.testclient import TestClient
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))
from main import app

client = TestClient(app)

SAMPLE_FAMILY_GROUP = {
    "group_id": "family_001",
    "name": "The Moyo Family",
    "group_type": "family",
    "members": [
        {"user_id": "parent_001", "role": "parent"},
        {"user_id": "child_001", "role": "child"}
    ],
    "features_enabled": ["shared_budget", "bill_reminders", "expense_splitting"]
}

SAMPLE_CHURCH_GROUP = {
    "group_id": "church_001",
    "name": "Grace Community Church",
    "group_type": "church",
    "members": [
        {"user_id": "treasurer_001", "role": "treasurer"},
        {"user_id": "auditor_001", "role": "auditor"},
        {"user_id": "member_001", "role": "member"}
    ],
    "features_enabled": ["contributions", "shared_expenses", "financial_reports"]
}

def test_health_check():
    response = client.get("/health")
    assert response.status_code == 200
    assert response.json()["status"] == "healthy"

def test_create_family_group():
    response = client.post("/groups", json=SAMPLE_FAMILY_GROUP)
    assert response.status_code == 200
    data = response.json()
    assert data["group_id"] == "family_001"
    assert data["group_type"] == "family"
    assert len(data["members"]) == 2

def test_create_church_group():
    response = client.post("/groups", json=SAMPLE_CHURCH_GROUP)
    assert response.status_code == 200
    data = response.json()
    assert data["group_type"] == "church"
    roles = [m["role"] for m in data["members"]]
    assert "treasurer" in roles
    assert "auditor" in roles

def test_get_group():
    client.post("/groups", json=SAMPLE_FAMILY_GROUP)
    response = client.get("/groups/family_001")
    assert response.status_code == 200
    assert response.json()["name"] == "The Moyo Family"

def test_get_group_not_found():
    response = client.get("/groups/nonexistent_group")
    assert response.status_code == 404

def test_collect_contribution():
    client.post("/groups", json=SAMPLE_CHURCH_GROUP)
    response = client.post("/groups/church_001/contributions?amount=50.0&member_id=member_001")
    assert response.status_code == 200
    assert response.json()["status"] == "success"

def test_split_expense():
    client.post("/groups", json=SAMPLE_FAMILY_GROUP)
    response = client.post(
        "/groups/family_001/expenses/split?total_amount=120.0",
        json=["parent_001", "child_001"]
    )
    assert response.status_code == 200
    data = response.json()
    assert data["split_amount_per_person"] == 60.0
