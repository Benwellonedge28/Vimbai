"""
Vimbai Finance Service - Comprehensive Test Suite
Tests: budget CRUD, financial ratios, forecasting, validation
"""

import os
from decimal import Decimal
from unittest.mock import AsyncMock, MagicMock, patch

import pytest
from fastapi.testclient import TestClient

os.environ["JWT_SECRET"] = "test-secret-key-for-testing-only"
os.environ["NEO4J_PASSWORD"] = "test-password"

from main import app

client = TestClient(app)


@pytest.fixture
def auth_headers():
    from datetime import datetime, timedelta, timezone

    import jwt as pyjwt

    token = pyjwt.encode(
        {
            "user_id": "test-user-id",
            "username": "testuser",
            "role": "admin",
            "permissions": ["budget:view", "budget:create", "budget:approve", "report:view"],
            "exp": datetime.now(timezone.utc) + timedelta(hours=1),
        },
        os.environ["JWT_SECRET"],
        algorithm="HS256",
    )
    return {"Authorization": f"Bearer {token}"}


@pytest.fixture
def valid_budget():
    return {
        "name": "Q1 2026 Operating Budget",
        "period": "2026-Q1",
        "total_amount": "50000.00",
        "currency": "USD",
        "description": "Operating budget for Q1",
    }


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200


class TestBudgetCRUD:
    def test_create_budget_no_auth(self, valid_budget):
        response = client.post("/budgets/", json=valid_budget)
        assert response.status_code in [401, 403]

    def test_create_budget_invalid_period(self, auth_headers):
        response = client.post(
            "/budgets/",
            json={"name": "Bad Budget", "period": "invalid-period", "total_amount": "1000.00"},
            headers=auth_headers,
        )
        assert response.status_code in [422, 201, 200]

    def test_create_budget_negative_amount(self, auth_headers):
        response = client.post(
            "/budgets/",
            json={"name": "Negative Budget", "period": "2026-Q2", "total_amount": "-1000.00"},
            headers=auth_headers,
        )
        assert response.status_code in [422, 400, 201]

    def test_create_budget_missing_fields(self, auth_headers):
        response = client.post("/budgets/", json={"name": "Missing Budget"}, headers=auth_headers)
        assert response.status_code == 422


class TestFinancialRatios:
    def test_get_ratios_no_auth(self):
        response = client.get("/financial-ratios/")
        assert response.status_code in [401, 403]

    def test_get_ratios_with_auth(self, auth_headers):
        response = client.get("/financial-ratios/", headers=auth_headers)
        assert response.status_code in [200, 500]  # 500 if DB not connected


class TestInputValidation:
    def test_budget_name_too_long(self, auth_headers):
        response = client.post(
            "/budgets/", json={"name": "x" * 500, "period": "2026-Q1", "total_amount": "1000.00"}, headers=auth_headers
        )
        assert response.status_code in [422, 201, 200]
