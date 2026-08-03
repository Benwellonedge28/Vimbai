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

def test_sso_auth_success():
    response = client.post("/auth/sso", json={
        "organization_id": "org_enterprise_001",
        "idp_token": "valid_idp_token_from_okta_or_azure"
    })
    assert response.status_code == 200
    data = response.json()
    assert "vimbai_access_token" in data
    assert data["organization_id"] == "org_enterprise_001"
    assert "SSO Authentication successful" in data["message"]

def test_sso_auth_no_personal_data_retained():
    response = client.post("/auth/sso", json={
        "organization_id": "org_enterprise_002",
        "idp_token": "another_valid_idp_token_here"
    })
    assert response.status_code == 200
    data = response.json()
    # Ensure no personal info is returned beyond what's needed
    forbidden_fields = ["email", "phone", "address", "salary", "date_of_birth"]
    for field in forbidden_fields:
        assert field not in data, f"Field '{field}' should not be returned from SSO"

def test_sso_auth_invalid_token():
    response = client.post("/auth/sso", json={
        "organization_id": "org_001",
        "idp_token": "short"
    })
    assert response.status_code == 401

def test_sso_auth_empty_token():
    response = client.post("/auth/sso", json={
        "organization_id": "org_001",
        "idp_token": ""
    })
    assert response.status_code == 401
