"""Book-scoping and persistence tests for financial-identity-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from financial_identity_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("fi_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession

_fake_session = FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "fi-user-1", "fi-user-2"
BOOK_A, BOOK_B = "fi-book-a", "fi-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _profile_payload(subject="subject-1", name="John Doe", **kw):
    payload = {"user_id": subject, "legal_name": name}
    payload.update(kw)
    return payload


def test_create_get_persist():
    resp = client.post("/profiles", json=_profile_payload(national_id="ID123", email="j@example.com"), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["verification_status"] == "pending"
    assert data["legal_name"] == "John Doe"

    stored = client.get(f"/profiles/{data['id']}", headers=H1).json()
    assert stored["national_id"] == "ID123"
    # persists across requests
    assert client.get(f"/profiles/{data['id']}", headers=H1).json()["legal_name"] == "John Doe"
    # other user cannot read this KYC profile
    assert client.get(f"/profiles/{data['id']}", headers=H2).status_code == 404


def test_verify_semantics():
    pid = client.post("/profiles", json=_profile_payload(subject="s-v"), headers=H1).json()["id"]
    # single document: high risk, stays pending
    resp = client.put(f"/profiles/{pid}/verify", json=["passport"], headers=H1)
    assert resp.status_code == 200
    assert resp.json() == {"id": pid, "status": "pending", "risk_score": 80}
    # two documents: verified, low risk
    resp = client.put(f"/profiles/{pid}/verify", json=["passport", "utility_bill"], headers=H1)
    assert resp.json() == {"id": pid, "status": "verified", "risk_score": 20}
    # persists
    stored = client.get(f"/profiles/{pid}", headers=H1).json()
    assert stored["verification_status"] == "verified"
    assert stored["kyc_documents"] == ["passport", "utility_bill"]
    assert stored["verified_at"] is not None
    # cross-user verify 404
    assert client.put(f"/profiles/{pid}/verify", json=["a", "b"], headers=H2).status_code == 404
    # cross-Book verify 404
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.put(f"/profiles/{pid}/verify", json=["a", "b"], headers=other_book).status_code == 404


def test_get_by_user_scoped():
    client.post("/profiles", json=_profile_payload(subject="shared-subject"), headers=H1)
    client.post("/profiles", json=_profile_payload(subject="shared-subject"), headers=H2)
    # each caller only sees their own profile for the subject
    mine = client.get("/profiles/user/shared-subject", headers=H1).json()
    assert mine["user_id"] == "shared-subject"
    assert client.get("/profiles/user/shared-subject", headers=H2).json()["user_id"] == "shared-subject"
    # U3 (nobody) gets 404
    assert client.get("/profiles/user/shared-subject", headers={"X-User-Id": "fi-user-3"}).status_code == 404
    # unknown subject 404
    assert client.get("/profiles/user/ghost", headers=H1).status_code == 404


def test_book_a_b_isolation():
    client.post("/profiles", json=_profile_payload(subject="sub-a"), headers=H1)
    client.post(
        "/profiles",
        json=_profile_payload(subject="sub-b"),
        headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
    )
    # Book A sees only sub-a's profile
    assert client.get("/profiles/user/sub-a", headers=H1).status_code == 200
    assert client.get("/profiles/user/sub-b", headers=H1).status_code == 404
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/profiles/user/sub-b", headers=other_book).status_code == 200
    assert client.get("/profiles/user/sub-a", headers=other_book).status_code == 404
    # personal spans both
    assert client.get("/profiles/user/sub-a", headers=H1_PERSONAL).status_code == 200
    assert client.get("/profiles/user/sub-b", headers=H1_PERSONAL).status_code == 200


def test_x_user_id_required():
    assert client.post("/profiles", json=_profile_payload()).status_code in (401, 403, 422)
    assert client.get("/profiles/anything").status_code in (401, 403, 422)
