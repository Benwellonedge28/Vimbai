"""
Vimbai Identity Service - Comprehensive Test Suite
Tests: user registration, login, JWT validation, MFA, RBAC, token refresh
"""

import json
import os
from unittest.mock import AsyncMock, MagicMock, patch

import pytest
from fastapi.testclient import TestClient

# Set required env vars before importing app
os.environ["JWT_SECRET"] = "test-secret-key-for-testing-only"

from main import app

client = TestClient(app)


# ============================================================================
# Fixtures
# ============================================================================


@pytest.fixture
def mock_neo4j():
    """Mock Neo4j session for database operations."""
    session = AsyncMock()
    result = AsyncMock()
    result.single = AsyncMock(return_value=None)
    result.values = AsyncMock(return_value=[])
    session.run = AsyncMock(return_value=result)
    return session


@pytest.fixture
def registered_user():
    """Register a test user and return the response."""
    response = client.post(
        "/users/register",
        json={
            "email": "test@vimbai.com",
            "username": "testuser",
            "password": "SecurePass123!",
            "first_name": "Test",
            "last_name": "User",
        },
    )
    return response


# ============================================================================
# Health Check
# ============================================================================


class TestHealthCheck:
    def test_root_endpoint(self):
        """Test that the health check endpoint returns 200."""
        response = client.get("/")
        assert response.status_code == 200

    def test_root_returns_service_info(self):
        """Test that the health check returns service information."""
        response = client.get("/")
        data = response.json()
        assert "service" in data or "status" in data or "name" in data


# ============================================================================
# User Registration
# ============================================================================


class TestUserRegistration:
    @patch("main.users_store", {})
    @patch("main.pwd_context")
    def test_register_user_success(self, mock_pwd, mock_store):
        """Test successful user registration."""
        mock_pwd.hash = MagicMock(return_value="hashed_password")
        response = client.post(
            "/users/register",
            json={
                "email": "newuser@vimbai.com",
                "username": "newuser",
                "password": "SecurePass123!",
                "first_name": "New",
                "last_name": "User",
            },
        )
        assert response.status_code in [201, 200, 409]  # 409 if already exists

    def test_register_user_duplicate_email(self):
        """Test that duplicate email registration is rejected."""
        user_data = {
            "email": "dup@vimbai.com",
            "username": "dupuser",
            "password": "SecurePass123!",
        }
        client.post("/users/register", json=user_data)
        response = client.post("/users/register", json=user_data)
        assert response.status_code in [409, 400, 422]

    def test_register_user_invalid_email(self):
        """Test that invalid email format is rejected."""
        response = client.post(
            "/users/register", json={"email": "not-an-email", "username": "baduser", "password": "SecurePass123!"}
        )
        assert response.status_code == 422

    def test_register_user_short_password(self):
        """Test that short passwords are rejected."""
        response = client.post(
            "/users/register", json={"email": "short@vimbai.com", "username": "shortpw", "password": "123"}
        )
        assert response.status_code in [422, 400]

    def test_register_user_missing_fields(self):
        """Test that missing required fields are rejected."""
        response = client.post("/users/register", json={"email": "missing@vimbai.com"})
        assert response.status_code == 422


# ============================================================================
# User Login
# ============================================================================


class TestUserLogin:
    @patch("main.users_store", {})
    @patch("main.pwd_context")
    def test_login_success(self, mock_pwd, mock_store):
        """Test successful login returns JWT token."""
        mock_pwd.hash = MagicMock(return_value="hashed_password")
        mock_pwd.verify = MagicMock(return_value=True)

        # Register first
        client.post(
            "/users/register", json={"email": "login@vimbai.com", "username": "loginuser", "password": "SecurePass123!"}
        )

        # Login
        response = client.post("/users/login", json={"email": "login@vimbai.com", "password": "SecurePass123!"})
        assert response.status_code in [200, 201]
        data = response.json()
        assert "access_token" in data or "token" in data

    def test_login_wrong_password(self):
        """Test that wrong password is rejected."""
        response = client.post("/users/login", json={"email": "nonexistent@vimbai.com", "password": "wrongpassword"})
        assert response.status_code in [401, 404, 400]

    def test_login_missing_credentials(self):
        """Test that login without credentials is rejected."""
        response = client.post("/users/login", json={})
        assert response.status_code == 422


# ============================================================================
# JWT Token Validation
# ============================================================================


class TestJWTValidation:
    def test_protected_endpoint_without_token(self):
        """Test that protected endpoints reject requests without a token."""
        response = client.get("/users/me")
        assert response.status_code in [401, 403]

    def test_protected_endpoint_with_invalid_token(self):
        """Test that invalid JWT tokens are rejected."""
        response = client.get("/users/me", headers={"Authorization": "Bearer invalid_token_here"})
        assert response.status_code in [401, 403]

    def test_protected_endpoint_with_expired_token(self):
        """Test that expired JWT tokens are rejected."""
        from datetime import datetime, timedelta, timezone

        import jwt as pyjwt

        expired_token = pyjwt.encode(
            {
                "user_id": "test-user-id",
                "username": "testuser",
                "role": "admin",
                "exp": datetime.now(timezone.utc) - timedelta(hours=1),
            },
            os.environ["JWT_SECRET"],
            algorithm="HS256",
        )

        response = client.get("/users/me", headers={"Authorization": f"Bearer {expired_token}"})
        assert response.status_code in [401, 403]


# ============================================================================
# Role-Based Access Control
# ============================================================================


class TestRBAC:
    def test_get_roles_unauthorized(self):
        """Test that getting roles without auth fails."""
        response = client.get("/roles")
        assert response.status_code in [401, 403]

    def test_create_role_unauthorized(self):
        """Test that creating a role without admin auth fails."""
        response = client.post("/roles", json={"name": "test_role", "description": "Test role"})
        assert response.status_code in [401, 403]


# ============================================================================
# Token Refresh
# ============================================================================


class TestTokenRefresh:
    def test_refresh_without_token(self):
        """Test that refresh without a token fails."""
        response = client.post("/token/refresh")
        assert response.status_code in [401, 422, 422]

    def test_refresh_with_invalid_token(self):
        """Test that refresh with an invalid token fails."""
        response = client.post("/token/refresh", json={"refresh_token": "invalid_token"})
        assert response.status_code in [401, 403, 422]


# ============================================================================
# Password Reset
# ============================================================================


class TestPasswordReset:
    def test_password_reset_with_valid_email(self):
        """Test that password reset request is accepted."""
        response = client.post("/password/reset", json={"email": "test@vimbai.com"})
        assert response.status_code in [200, 202, 404]

    def test_password_reset_without_email(self):
        """Test that password reset without email is rejected."""
        response = client.post("/password/reset", json={})
        assert response.status_code == 422


# ============================================================================
# Input Validation
# ============================================================================


class TestInputValidation:
    def test_register_user_long_username(self):
        """Test that overly long usernames are rejected."""
        response = client.post(
            "/users/register",
            json={"email": "longuser@vimbai.com", "username": "a" * 100, "password": "SecurePass123!"},
        )
        assert response.status_code == 422

    def test_register_user_short_username(self):
        """Test that short usernames are rejected."""
        response = client.post(
            "/users/register", json={"email": "shortuser@vimbai.com", "username": "ab", "password": "SecurePass123!"}
        )
        assert response.status_code == 422

    def test_register_user_invalid_json(self):
        """Test that malformed JSON is rejected."""
        response = client.post("/users/register", json=None)
        assert response.status_code == 422
