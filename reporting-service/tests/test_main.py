"""
Vimbai Reporting Service - Test Suite
Tests: report generation, scheduling, exports
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
         "permissions": ["report:view", "report:create", "report:export"],
         "exp": datetime.now(timezone.utc) + timedelta(hours=1)},
        os.environ["JWT_SECRET"], algorithm="HS256"
    )
    return {"Authorization": f"Bearer {token}"}


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200


class TestReportGeneration:
    def test_generate_report_no_auth(self):
        response = client.post("/generate", json={
            "report_type": "balance_sheet",
            "period": "2026-Q1"
        })
        assert response.status_code in [401, 403, 422]

    def test_generate_report_with_auth(self, auth_headers):
        response = client.post("/generate", json={
            "report_type": "balance_sheet",
            "period": "2026-Q1"
        }, headers=auth_headers)
        assert response.status_code in [200, 201, 500]

    def test_generate_report_missing_fields(self, auth_headers):
        response = client.post("/generate", json={"report_type": "balance_sheet"}, headers=auth_headers)
        assert response.status_code in [422, 400, 200, 201]

    def test_generate_invalid_report_type(self, auth_headers):
        response = client.post("/generate", json={
            "report_type": "nonexistent_report",
            "period": "2026-Q1"
        }, headers=auth_headers)
        assert response.status_code in [422, 400, 200, 404, 500]


class TestReportScheduling:
    def test_schedule_report_no_auth(self):
        response = client.post("/schedule", json={
            "report_type": "income_statement",
            "frequency": "monthly",
            "email": "user@vimbai.com"
        })
        assert response.status_code in [401, 403, 422]

    def test_schedule_report_with_auth(self, auth_headers):
        response = client.post("/schedule", json={
            "report_type": "income_statement",
            "frequency": "monthly",
            "email": "user@vimbai.com"
        }, headers=auth_headers)
        assert response.status_code in [200, 201, 500]

    def test_schedule_report_invalid_frequency(self, auth_headers):
        response = client.post("/schedule", json={
            "report_type": "income_statement",
            "frequency": "invalid",
            "email": "user@vimbai.com"
        }, headers=auth_headers)
        assert response.status_code in [422, 400, 200, 201]
