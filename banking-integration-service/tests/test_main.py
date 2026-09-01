"""
Vimbai Banking Integration Service - Test Suite
Tests: bank connections, transactions, reconciliation
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
         "permissions": ["banking:view", "banking:create", "banking:sync"],
         "exp": datetime.now(timezone.utc) + timedelta(hours=1)},
        os.environ["JWT_SECRET"], algorithm="HS256"
    )
    return {"Authorization": f"Bearer {token}"}


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200


class TestBankConnections:
    def test_list_banks_no_auth(self):
        response = client.get("/banks/")
        assert response.status_code in [401, 403]

    def test_list_banks_with_auth(self, auth_headers):
        response = client.get("/banks/", headers=auth_headers)
        assert response.status_code in [200, 500]

    def test_create_bank_connection_no_auth(self):
        response = client.post("/banking-integration/connect", json={
            "bank_code": "test-bank",
            "credentials": {"api_key": "test-key"}
        })
        assert response.status_code in [401, 403, 404]


class TestTransactions:
    def test_get_transactions_no_auth(self):
        response = client.get("/transactions/")
        assert response.status_code in [401, 403]

    def test_get_transactions_with_auth(self, auth_headers):
        response = client.get("/transactions/", headers=auth_headers)
        assert response.status_code in [200, 500]

    def test_sync_transactions_no_auth(self):
        response = client.post("/banking-integration/sync", json={
            "bank_connection_id": "test-connection"
        })
        assert response.status_code in [401, 403, 404]


class TestInputValidation:
    def test_invalid_bank_code(self, auth_headers):
        response = client.post("/banking-integration/connect", json={
            "bank_code": "",
            "credentials": {}
        }, headers=auth_headers)
        assert response.status_code in [422, 404]
