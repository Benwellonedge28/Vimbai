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

import main
from main import app

from tests.conftest import _fake_session  # noqa: E402

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
    def test_register_user_success(self):
        """Test successful user registration."""
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
    def test_login_success(self):
        """Test successful login returns JWT token."""
        # Register first
        client.post(
            "/users/register", json={"email": "login@vimbai.com", "username": "loginuser", "password": "SecurePass123!"}
        )

        # Login
        response = client.post("/users/login", data={"username": "login@vimbai.com", "password": "SecurePass123!"})
        assert response.status_code in [200, 201]
        data = response.json()
        assert "access_token" in data or "token" in data

    def test_login_wrong_password(self):
        """Test that wrong password is rejected."""
        response = client.post("/users/login", data={"username": "nonexistent@vimbai.com", "password": "wrongpassword"})
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


def _register_and_token(email="rbac-user@vimbai.com", role_ids=None):
    """Register a fresh user holding the given roles and return auth headers."""
    from datetime import datetime, timedelta, timezone

    import jwt as pyjwt

    resp = client.post(
        "/users/register",
        json={
            "email": email,
            "username": email.split("@")[0],
            "password": "SecurePass123!",
            "role_ids": role_ids or [],
        },
    )
    assert resp.status_code in [200, 201], resp.text
    user_id = resp.json()["id"]
    token = pyjwt.encode(
        {"sub": user_id, "exp": datetime.now(timezone.utc) + timedelta(hours=1)},
        os.environ["JWT_SECRET"],
        algorithm="HS256",
    )
    return {"Authorization": f"Bearer {token}"}


class TestRBACSecurity:
    """Regression tests: role management MUST be authenticated (was fully open)."""

    def test_roles_require_authentication(self):
        assert client.get("/roles").status_code == 401
        assert client.post("/roles", json={"name": "x", "description": "x"}).status_code == 401
        assert client.get("/roles/admin").status_code == 401
        assert client.put("/roles/admin", json={"name": "x", "description": "x"}).status_code == 401

    def test_admin_can_create_and_update_roles(self):
        headers = _register_and_token("rbac-admin@vimbai.com", ["admin"])
        created = client.post(
            "/roles", headers=headers, json={"name": "custom_role", "description": "Custom", "permissions": []}
        )
        assert created.status_code == 201, created.text
        role_id = created.json()["id"]
        updated = client.put(
            "/roles/" + role_id,
            headers=headers,
            json={"name": "custom_role2", "description": "Custom2", "permissions": []},
        )
        assert updated.status_code == 200
        assert updated.json()["name"] == "custom_role2"

    def test_non_admin_cannot_create_roles(self):
        headers = _register_and_token("rbac-viewer@vimbai.com", ["viewer"])
        resp = client.post("/roles", headers=headers, json={"name": "nope", "description": "nope", "permissions": []})
        assert resp.status_code == 403


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


# ============================================================================
# Identity Record Persistence + Access Control (fake Neo4j harness)
# ============================================================================


def _register_and_login(email, username, password="SecurePass123!"):
    """Register a user and return (user_id, auth headers) via the real login flow."""
    resp = client.post("/users/register", json={"email": email, "username": username, "password": password})
    assert resp.status_code == 201, resp.text
    user_id = resp.json()["id"]
    login = client.post("/users/login", data={"username": email, "password": password})
    assert login.status_code == 200, login.text
    return user_id, {"Authorization": f"Bearer {login.json()['access_token']}"}


class TestPersistence:
    """Durable identity records must live in the graph, not in module dicts."""

    def test_registered_user_persisted_as_node(self):
        uid, _ = _register_and_login("persist@vimbai.com", "persistuser")
        node = next((n for n in _fake_session.nodes if n["label"] == "IdentityUser" and n["props"]["id"] == uid), None)
        assert node is not None
        assert node["props"]["email"] == "persist@vimbai.com"

    def test_update_persists(self):
        uid, headers = _register_and_login("upd@vimbai.com", "upduser")
        resp = client.put(f"/users/{uid}", headers=headers, json={"first_name": "Upd"})
        assert resp.status_code == 200, resp.text
        node = next(n for n in _fake_session.nodes if n["label"] == "IdentityUser" and n["props"]["id"] == uid)
        assert node["props"]["first_name"] == "Upd"

    def test_audit_logs_persisted_as_nodes(self):
        _, headers = _register_and_login("aud@vimbai.com", "auduser")
        node = next(
            (
                n
                for n in _fake_session.nodes
                if n["label"] == "IdentityAuditLog" and n["props"]["action"] == "user.registered"
            ),
            None,
        )
        assert node is not None

    def test_roles_seeded_once(self):
        _, headers = _register_and_login("roleseed@vimbai.com", "roleseeduser")
        resp = client.get("/roles", headers=headers)
        assert resp.status_code == 200
        role_ids = {r["id"] for r in resp.json()["roles"]}
        assert {"admin", "accountant", "viewer"} <= role_ids
        # Seeding is idempotent
        resp2 = client.get("/roles", headers=headers)
        assert len(resp2.json()["roles"]) == len(resp.json()["roles"])


class TestUserPrivacy:
    """User records are private: self or USER_MANAGE admin only."""

    def test_get_user_requires_auth(self):
        assert client.get("/users/anyone").status_code == 401

    def test_self_can_read_their_record(self):
        uid, headers = _register_and_login("selfread@vimbai.com", "selfread")
        resp = client.get(f"/users/{uid}", headers=headers)
        assert resp.status_code == 200
        assert resp.json()["email"] == "selfread@vimbai.com"

    def test_foreign_user_gets_404(self):
        uid_a, _ = _register_and_login("priv-a@vimbai.com", "priva")
        _, headers_b = _register_and_login("priv-b@vimbai.com", "privb")
        resp = client.get(f"/users/{uid_a}", headers=headers_b)
        assert resp.status_code == 404

    def test_admin_can_read_any_user(self):
        uid_a, _ = _register_and_login("adm-a@vimbai.com", "adma")
        admin_headers = _register_and_token("adm-admin3@vimbai.com", ["admin"])
        assert client.get(f"/users/{uid_a}", headers=admin_headers).status_code == 200

    def test_update_requires_auth(self):
        assert client.put("/users/anyone", json={"first_name": "x"}).status_code == 401

    def test_cross_user_update_404(self):
        uid_a, _ = _register_and_login("xup-a@vimbai.com", "xupa")
        _, headers_b = _register_and_login("xup-b@vimbai.com", "xupb")
        resp = client.put(f"/users/{uid_a}", headers=headers_b, json={"first_name": "Hacked"})
        assert resp.status_code == 404

    def test_change_password_self_only(self):
        uid_a, _ = _register_and_login("cpw-a@vimbai.com", "cpwa")
        _, headers_b = _register_and_login("cpw-b@vimbai.com", "cpwb")
        resp = client.post(
            f"/users/{uid_a}/change-password",
            headers=headers_b,
            json={"current_password": "SecurePass123!", "new_password": "NewPass456!"},
        )
        assert resp.status_code == 403

    def test_change_password_correct_flow(self):
        uid, headers = _register_and_login("cpw-ok@vimbai.com", "cpwok")
        resp = client.post(
            f"/users/{uid}/change-password",
            headers=headers,
            json={"current_password": "SecurePass123!", "new_password": "NewPass456!"},
        )
        assert resp.status_code == 200
        assert (
            client.post("/users/login", data={"username": "cpw-ok@vimbai.com", "password": "NewPass456!"}).status_code
            == 200
        )


class TestAuditLogAccess:
    def test_requires_auth(self):
        assert client.get("/audit-logs").status_code == 401

    def test_non_admin_forbidden(self):
        _, headers = _register_and_login("audlog-viewer@vimbai.com", "audlogviewer")
        assert client.get("/audit-logs", headers=headers).status_code == 403

    def test_admin_can_list(self):
        admin_headers = _register_and_token("audlog-admin@vimbai.com", ["admin"])
        res = client.get("/audit-logs", headers=admin_headers)
        assert res.status_code == 200
        assert res.json()["total"] >= 1
        # filterable
        res2 = client.get("/audit-logs", headers=admin_headers, params={"action": "user.registered"})
        assert all(log["action"] == "user.registered" for log in res2.json()["logs"])


class TestSessionPrivacy:
    def test_requires_auth(self):
        assert client.get("/sessions").status_code == 401

    def test_only_own_sessions_listed(self):
        _, h_a = _register_and_login("sess-a@vimbai.com", "sessa")
        _, h_b = _register_and_login("sess-b@vimbai.com", "sessb")
        res_a = client.get("/sessions", headers=h_a)
        assert res_a.status_code == 200
        user_a_id = res_a.json()["sessions"][0]["user_id"]
        assert all(s["user_id"] == user_a_id for s in res_a.json()["sessions"])
        # a's listing must not contain b's session
        res_b = client.get("/sessions", headers=h_b)
        ids_a = {s["session_id"] for s in res_a.json()["sessions"]}
        ids_b = {s["session_id"] for s in res_b.json()["sessions"]}
        assert not (ids_a & ids_b)

    def test_cannot_revoke_foreign_session(self):
        _, h_a = _register_and_login("rev-a@vimbai.com", "reva")
        _, h_b = _register_and_login("rev-b@vimbai.com", "revb")
        foreign_sid = client.get("/sessions", headers=h_b).json()["sessions"][0]["session_id"]
        assert client.delete(f"/sessions/{foreign_sid}", headers=h_a).status_code == 404

    def test_can_revoke_own_session(self):
        _, h_a = _register_and_login("rev-own@vimbai.com", "revown")
        sid = client.get("/sessions", headers=h_a).json()["sessions"][0]["session_id"]
        assert client.delete(f"/sessions/{sid}", headers=h_a).status_code == 200


class TestOrganizationAccess:
    def test_create_requires_auth(self):
        assert (
            client.post(
                "/organizations", json={"name": "X", "description": "x", "admin_email": "a@vimbai.com"}
            ).status_code
            == 401
        )

    def test_member_and_admin_access(self):
        _, headers = _register_and_login("org-user@vimbai.com", "orguser")
        created = client.post(
            "/organizations", headers=headers, json={"name": "Acme", "description": "d", "admin_email": "o@vimbai.com"}
        )
        assert created.status_code == 201, created.text
        org_id = created.json()["id"]
        # creator is not a member (organization_id not set) -> 404 for themselves
        assert client.get(f"/organizations/{org_id}", headers=headers).status_code == 404

    def test_membership_grants_read(self):
        member_headers = _register_and_token("member@vimbai.com")
        admin_headers = _register_and_token("orgadmin@vimbai.com", ["admin"])
        created = client.post(
            "/organizations",
            headers=admin_headers,
            json={"name": "Beta", "description": "d", "admin_email": "o@vimbai.com"},
        )
        org_id = created.json()["id"]
        # non-member user cannot read org
        assert client.get(f"/organizations/{org_id}", headers=member_headers).status_code == 404
        # admin can
        assert client.get(f"/organizations/{org_id}", headers=admin_headers).status_code == 200


class TestMFAFlow:
    def test_setup_and_login_with_mfa(self):
        uid, headers = _register_and_login("mfa@vimbai.com", "mfauser")
        resp = client.post(f"/users/{uid}/mfa/setup", headers=headers, json={"method": "email"})
        assert resp.status_code == 200, resp.text
        code = next(c for c, d in main.mfa_codes.items() if d["user_id"] == uid)
        verify = client.post(f"/users/{uid}/mfa/verify", headers=headers, json={"code": code, "method": "email"})
        assert verify.status_code == 200, verify.text
        # login now requires MFA
        login = client.post("/users/login", data={"username": "mfa@vimbai.com", "password": "SecurePass123!"})
        assert login.status_code == 200
        assert login.json()["mfa_required"] is True
        # complete MFA login
        temp_token = login.json()["temp_token"]
        code2 = next(c for c, d in main.mfa_codes.items() if d["user_id"] == uid)
        done = client.post("/users/login/verify-mfa", json={"code": code2, "method": "email", "temp_token": temp_token})
        assert done.status_code == 200, done.text
        assert "access_token" in done.json()

    def test_setup_requires_auth(self):
        uid, _ = _register_and_login("mfa-open@vimbai.com", "mfaopen")
        assert client.post(f"/users/{uid}/mfa/setup", json={"method": "email"}).status_code == 401

    def test_foreign_user_cannot_setup_mfa(self):
        uid_a, _ = _register_and_login("mfa-x-a@vimbai.com", "mfaxa")
        _, headers_b = _register_and_login("mfa-x-b@vimbai.com", "mfaxb")
        resp = client.post(f"/users/{uid_a}/mfa/setup", headers=headers_b, json={"method": "email"})
        assert resp.status_code == 404
