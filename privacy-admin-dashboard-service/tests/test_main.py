"""Privacy admin: Book-scoped Neo4j persistence.

Privacy requests and consent records are audit material: they must survive a
restart and must never be visible across tenants. They lived in module-level
lists, so neither held; these tests pin the persisted contract.
"""

import privacy_admin_store as store
import pytest
from fake_neo4j import FakeDriver, FakeSession
from fastapi.testclient import TestClient
from main import app

BOOK_ONE = {"X-User-Id": "user-1", "X-Book-ID": "book-1"}
BOOK_TWO = {"X-User-Id": "user-2", "X-Book-ID": "book-2"}


@pytest.fixture
def client():
    store.set_driver(FakeDriver(FakeSession()))
    yield TestClient(app)
    store.set_driver(None)


def _request(client, headers=BOOK_ONE, request_type="access"):
    resp = client.post(
        "/requests",
        params={
            "request_type": request_type,
            "subject_name": "Tariro Moyo",
            "subject_email": "tariro@example.com",
            "notes": "dsar",
        },
        headers=headers,
    )
    assert resp.status_code == 200, resp.text
    return resp.json()


def test_health(client):
    assert client.get("/health").json()["service"] == "privacy-admin-dashboard-service"


def test_request_round_trips(client):
    created = _request(client)
    assert created["status"] == "pending"
    listed = client.get("/requests", headers=BOOK_ONE).json()
    assert [r["id"] for r in listed] == [created["id"]]


def test_invalid_request_type_is_rejected(client):
    resp = client.post(
        "/requests",
        params={"request_type": "erasure_plus", "subject_name": "x", "subject_email": "x@y.z"},
        headers=BOOK_ONE,
    )
    assert resp.status_code == 400


def test_resolving_stamps_resolved_at_and_persists(client):
    created = _request(client)
    updated = client.put(f"/requests/{created['id']}", json={"status": "resolved", "notes": "done"}, headers=BOOK_ONE)
    assert updated.status_code == 200, updated.text
    assert updated.json()["resolved_at"] is not None
    again = client.get("/requests", params={"status": "resolved"}, headers=BOOK_ONE).json()
    assert [r["id"] for r in again] == [created["id"]]


def test_consent_revocation_is_persisted(client):
    record = client.post(
        "/consent", params={"subject_email": "tariro@example.com", "data_type": "marketing"}, headers=BOOK_ONE
    ).json()
    assert len(client.get("/consent/tariro@example.com", headers=BOOK_ONE).json()) == 1
    assert client.delete(f"/consent/{record['id']}", headers=BOOK_ONE).json()["revoked"] is True
    assert client.get("/consent/tariro@example.com", headers=BOOK_ONE).json() == []
    dashboard = client.get("/dashboard", headers=BOOK_ONE).json()
    assert dashboard["revoked_consents"] == 1


def test_one_book_never_sees_another_books_privacy_records(client):
    mine = _request(client, BOOK_ONE)
    client.post("/consent", params={"subject_email": "tariro@example.com", "data_type": "analytics"}, headers=BOOK_ONE)
    assert client.get("/requests", headers=BOOK_TWO).json() == []
    assert client.get("/consent/tariro@example.com", headers=BOOK_TWO).json() == []
    assert client.put(f"/requests/{mine['id']}", json={"status": "rejected"}, headers=BOOK_TWO).status_code == 404
    assert client.delete(f"/requests/{mine['id']}", headers=BOOK_TWO).status_code == 404
    other_dashboard = client.get("/dashboard", headers=BOOK_TWO).json()
    assert other_dashboard["total_requests"] == 0
    assert client.get("/dashboard", headers=BOOK_ONE).json()["total_requests"] == 1
