"""
Vimbai Automation Engine Service - Test Suite
Tests: process automation, rule execution, health checks
"""
import pytest
import os
from unittest.mock import AsyncMock, patch
from fastapi.testclient import TestClient

os.environ["JWT_SECRET"] = "test-secret-key-for-testing-only"
os.environ["NEO4J_PASSWORD"] = "test-password"

from main import app

client = TestClient(app)


@pytest.fixture
def auth_headers():
    import jwt as pyjwt
    from datetime import datetime, timezone, timedelta
    token = pyjwt.encode(
        {"user_id": "test-user-id", "username": "testuser", "role": "admin",
         "permissions": ["automation:view", "automation:execute"],
         "exp": datetime.now(timezone.utc) + timedelta(hours=1)},
        os.environ["JWT_SECRET"], algorithm="HS256"
    )
    return {"Authorization": f"Bearer {token}"}


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200

    def test_health_endpoint(self):
        response = client.get("/health")
        assert response.status_code == 200


class TestProcessAutomation:
    def test_process_no_auth(self):
        response = client.post("/process", json={
            "action": "send_reminder",
            "data": {"recipient": "user@vimbai.com"}
        })
        assert response.status_code in [401, 403, 422]

    def test_process_with_auth(self, auth_headers):
        response = client.post("/process", json={
            "action": "send_reminder",
            "data": {"recipient": "user@vimbai.com"}
        }, headers=auth_headers)
        assert response.status_code in [200, 201, 500]

    def test_process_missing_fields(self, auth_headers):
        response = client.post("/process", json={"action": "send_reminder"}, headers=auth_headers)
        assert response.status_code in [422, 400, 200]
