"""Banking integration: Book-scoped Neo4j persistence.

Connections and transactions were module-level lists. They already filtered by
Book in Python, but a restart dropped every connection and the account number
uniqueness check silently reset. They now live in Neo4j, caller-owned.
"""

import banking_integration_store as store
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


def _connect(client, headers=BOOK_ONE, bank="Standard Bank", account="1234567890"):
    resp = client.post(
        "/connect",
        params={"bank_name": bank, "account_number": account, "api_key": "test-key", "account_type": "checking"},
        headers=headers,
    )
    assert resp.status_code == 200, resp.text
    return resp.json()


def test_health(client):
    assert client.get("/health").json()["service"] == "banking-integration-service"


def test_connection_round_trips(client):
    conn = _connect(client)
    assert conn["status"] == "active"
    listed = client.get("/connections", headers=BOOK_ONE).json()
    assert [c["id"] for c in listed] == [conn["id"]]


def test_duplicate_account_in_the_same_book_is_a_conflict(client):
    _connect(client)
    again = client.post(
        "/connect",
        params={"bank_name": "Standard Bank", "account_number": "1234567890", "api_key": "k"},
        headers=BOOK_ONE,
    )
    assert again.status_code == 409


def test_same_account_can_be_connected_in_another_book(client):
    _connect(client, BOOK_ONE)
    other = _connect(client, BOOK_TWO)
    assert other["book_id"] == "book-2"


def test_sync_stamps_last_sync(client):
    conn = _connect(client)
    synced = client.post(f"/sync/{conn['id']}", headers=BOOK_ONE)
    assert synced.status_code == 200
    assert synced.json()["status"] == "success"
    listed = client.get("/connections", headers=BOOK_ONE).json()
    assert listed[0]["last_sync"] is not None


def test_disconnect_persists(client):
    conn = _connect(client)
    assert client.delete(f"/connections/{conn['id']}", headers=BOOK_ONE).json()["status"] == "disconnected"
    assert client.get("/connections", headers=BOOK_ONE).json()[0]["status"] == "disconnected"
    blocked = client.post(f"/sync/{conn['id']}", headers=BOOK_ONE)
    assert blocked.status_code == 400


def test_one_book_never_sees_another_books_connections(client):
    mine = _connect(client, BOOK_ONE)
    assert client.get("/connections", headers=BOOK_TWO).json() == []
    assert client.post(f"/sync/{mine['id']}", headers=BOOK_TWO).status_code == 404
    assert client.get(f"/transactions/{mine['id']}", headers=BOOK_TWO).status_code == 404
    assert client.delete(f"/connections/{mine['id']}", headers=BOOK_TWO).status_code == 404
