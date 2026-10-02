"""Caller-identity scoping and persistence tests for mfa-auth-service (fake Neo4j harness).

MFA has no Book dimension: the user's second factor belongs to them
across all Books, so the guard here is caller identity (X-User-Id),
not Book gating.
"""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from mfa_auth_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("mfa_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    main._pending_challenges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    main._pending_challenges.clear()


U1, U2 = "mfa-user-1", "mfa-user-2"
H1 = {"X-User-Id": U1}
H2 = {"X-User-Id": U2}


def test_setup_and_verify_self():
    r = client.post("/setup", json={"user_id": U1, "method": "totp"}, headers=H1)
    assert r.status_code == 200, r.text
    data = r.json()
    assert data["user_id"] == U1
    assert data["secret"]
    assert len(data["backup_codes"]) == 8
    assert data["qr_uri"].startswith("otpauth://totp/Vimbai:")

    # verify with a 6-digit code succeeds (simplified TOTP semantics preserved)
    v = client.post("/verify", json={"user_id": U1, "code": "123456"}, headers=H1)
    assert v.status_code == 200
    assert v.json()["verified"] is True
    assert v.json()["access_token"]

    # bad code shape fails
    v = client.post("/verify", json={"user_id": U1, "code": "12"}, headers=H1)
    assert v.json()["verified"] is False
    assert v.json()["message"] == "Invalid MFA code"


def test_setup_overwrites_own_secret():
    s1 = client.post("/setup", json={"user_id": U1}, headers=H1).json()
    s2 = client.post("/setup", json={"user_id": U1}, headers=H1).json()
    assert s1["secret"] != s2["secret"]
    # still exactly one enrollment; verification uses the latest secret
    assert client.post("/verify", json={"user_id": U1, "code": "123456"}, headers=H1).json()["verified"] is True


def test_cross_user_blocked():
    # enrolling/verifying on BEHALF of another user is now 403
    assert client.post("/setup", json={"user_id": U2, "method": "totp"}, headers=H1).status_code == 403
    assert client.post("/verify", json={"user_id": U2, "code": "123456"}, headers=H1).status_code == 403
    # and U2's enrollment is untouched / not set up
    r = client.post("/verify", json={"user_id": U2, "code": "123456"}, headers=H2)
    assert r.json()["message"] == "MFA not set up for this user"
    # challenge creation for someone else is 403 too
    assert client.post("/challenge", params={"user_id": U2}, headers=H1).status_code == 403


def test_challenge_flow_owned_by_caller():
    r = client.post("/challenge", params={"user_id": U1, "method": "sms"}, headers=H1)
    assert r.status_code == 200
    data = r.json()
    assert "challenge_id" in data
    assert data["expires_in_minutes"] == 5

    # wrong code -> verified False (challenge stays pending)
    assert (
        client.post(f"/challenge/{data['challenge_id']}/verify", params={"code": "000000"}, headers=H1).json()[
            "verified"
        ]
        is False
    )

    # another caller must not even see the challenge (404, no leak)
    assert (
        client.post(f"/challenge/{data['challenge_id']}/verify", params={"code": "000000"}, headers=H2).status_code
        == 404
    )
    assert client.post(f"/challenge/{data['challenge_id']}/verify", params={"code": "000000"}).status_code in (
        401,
        403,
        422,
    )

    # correct code (read from the store) verifies and consumes the challenge
    ch = main._pending_challenges[data["challenge_id"]]
    r = client.post(f"/challenge/{data['challenge_id']}/verify", params={"code": ch["code"]}, headers=H1)
    assert r.json() == {"verified": True, "user_id": U1}
    assert (
        client.post(f"/challenge/{data['challenge_id']}/verify", params={"code": ch["code"]}, headers=H1).status_code
        == 404
    )


def test_enrollment_survives_new_session():
    """The TOTP secret persists in the store: a fresh session reads the same enrollment."""
    client.post("/setup", json={"user_id": U1}, headers=H1)
    secret = None
    for node in _fake_session.nodes:
        if node.get("label") == "MfaSecret":
            secret = node["props"]["secret"]
    assert secret
    # simulate restart: brand-new client on the same backing store
    client2 = TestClient(main.app)
    v = client2.post("/verify", json={"user_id": U1, "code": "123456"}, headers=H1)
    assert v.json()["verified"] is True


def test_health_and_auth():
    assert client.get("/health").json()["status"] == "healthy"
    r = client.get("/health", headers=H1)
    assert r.json()["enrolled_users"] == 0
    client.post("/setup", json={"user_id": U1}, headers=H1)
    assert client.get("/health", headers=H1).json()["enrolled_users"] == 1
    # endpoints require auth
    assert client.post("/setup", json={"user_id": U1}).status_code in (401, 403, 422)
    assert client.post("/verify", json={"user_id": U1, "code": "123456"}).status_code in (401, 403, 422)
    assert client.post("/challenge", params={"user_id": U1}).status_code in (401, 403, 422)
