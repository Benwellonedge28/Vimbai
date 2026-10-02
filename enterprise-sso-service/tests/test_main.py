import os
import sys

import pytest
from fastapi.testclient import TestClient

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from main import app

client = TestClient(app)


def test_health_check():
    response = client.get("/health")
    assert response.status_code == 200
    assert response.json()["status"] == "healthy"


def _jwt_like(sub):
    """Build a JWT-shaped IdP token (the current contract validates structure)."""
    import base64
    import json

    header = base64.urlsafe_b64encode(json.dumps({"alg": "RS256"}).encode()).rstrip(b"=").decode()
    payload = base64.urlsafe_b64encode(json.dumps({"sub": sub, "exp": 99999999999}).encode()).rstrip(b"=").decode()
    return f"{header}.{payload}.sig"


def test_sso_auth_success():
    response = client.post(
        "/auth/sso",
        json={"organization_id": "org_enterprise_001", "idp_token": _jwt_like("sso_user_1"), "provider": "oidc"},
    )
    assert response.status_code == 200
    data = response.json()
    assert "vimbai_access_token" in data
    assert data["organization_id"] == "org_enterprise_001"
    assert "SSO authentication successful" in data["message"]


def test_sso_auth_no_personal_data_retained():
    response = client.post(
        "/auth/sso",
        json={"organization_id": "org_enterprise_002", "idp_token": _jwt_like("sso_user_2"), "provider": "oidc"},
    )
    assert response.status_code == 200
    data = response.json()
    # Ensure no personal info is returned beyond what's needed
    forbidden_fields = ["email", "phone", "address", "salary", "date_of_birth"]
    for field in forbidden_fields:
        assert field not in data, f"Field '{field}' should not be returned from SSO"


def test_sso_auth_invalid_token():
    response = client.post("/auth/sso", json={"organization_id": "org_001", "idp_token": "short"})
    assert response.status_code == 401


def test_sso_auth_empty_token():
    response = client.post("/auth/sso", json={"organization_id": "org_001", "idp_token": ""})
    assert response.status_code == 401
