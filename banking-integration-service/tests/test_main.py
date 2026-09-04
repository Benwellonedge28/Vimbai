"""
Vimbai Banking Integration Service - Test Suite
Tests: bank connections, transactions, reconciliation, Book isolation.
"""

import os
from datetime import datetime, timedelta, timezone

import pytest
from fastapi.testclient import TestClient

os.environ.setdefault("JWT_SECRET", "test-secret-key-for-testing-only")
os.environ.setdefault("NEO4J_PASSWORD", "test-password")

import main as banking_main
from main import app

client = TestClient(app)

BOOK_A = {"X-Book-ID": "book-aaa-111"}
BOOK_B = {"X-Book-ID": "book-bbb-222"}


@pytest.fixture(autouse=True)
def clean_state():
    """Reset the in-memory stores between tests."""
    banking_main.connections.clear()
    banking_main.transactions.clear()
    yield
    banking_main.connections.clear()
    banking_main.transactions.clear()


def _connect(**overrides):
    payload = {
        "bank_name": "CBZ",
        "account_number": "0123456789",
        "api_key": "test-key",
        "account_type": "checking",
    }
    payload.update(overrides)
    return client.post("/connect", params=payload)


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200


class TestBankConnections:
    def test_connect_creates_connection(self):
        response = _connect()
        assert response.status_code == 200
        assert response.json()["bank_name"] == "CBZ"

    def test_connect_duplicate_rejected(self):
        _connect()
        response = _connect()
        assert response.status_code == 409

    def test_list_connections(self):
        _connect(bank_name="Stanbic")
        _connect(bank_name="Ecobank", account_number="999")
        response = client.get("/connections")
        assert response.status_code == 200
        assert len(response.json()) == 2

    def test_disconnect(self):
        conn_id = _connect().json()["id"]
        response = client.delete(f"/connections/{conn_id}")
        assert response.status_code == 200
        assert response.json()["status"] == "disconnected"

    def test_sync_unknown_connection_404(self):
        response = client.post("/sync/does-not-exist")
        assert response.status_code == 404


class TestBookIsolation:
    def test_connection_stamped_with_book_id(self):
        from fastapi.testclient import TestClient as TC

        r = TC(app).post(
            "/connect",
            params={"bank_name": "FBC", "account_number": "333", "api_key": "k"},
            headers=BOOK_A,
        )
        assert r.status_code == 200
        assert r.json()["book_id"] == "book-aaa-111"
        # unscoped (personal) connections keep book_id = None
        r2 = _connect(bank_name="FBC", account_number="334")
        assert r2.status_code == 200
        assert r2.json()["book_id"] is None

    def test_connections_isolated_between_books(self):
        # Book A creates a connection
        with client:
            r = TestClient(app).post(
                "/connect", params={"bank_name": "CBZ", "account_number": "111", "api_key": "k"}, headers=BOOK_A
            )
        assert r.status_code == 200

        # Book A sees it, Book B does not
        a = TestClient(app).get("/connections", headers=BOOK_A)
        b = TestClient(app).get("/connections", headers=BOOK_B)
        assert len(a.json()) == 1
        assert len(b.json()) == 0

    def test_same_account_allowed_in_different_books(self):
        # The duplicate check is Book-scoped, so two Books can each
        # connect the same bank account.
        r1 = TestClient(app).post(
            "/connect", params={"bank_name": "CBZ", "account_number": "777", "api_key": "k"}, headers=BOOK_A
        )
        r2 = TestClient(app).post(
            "/connect", params={"bank_name": "CBZ", "account_number": "777", "api_key": "k"}, headers=BOOK_B
        )
        assert r1.status_code == 200
        assert r2.status_code == 200

    def test_cross_book_sync_blocked(self):
        r = TestClient(app).post(
            "/connect", params={"bank_name": "CBZ", "account_number": "555", "api_key": "k"}, headers=BOOK_A
        )
        conn_id = r.json()["id"]
        # Book B cannot sync Book A's connection
        response = TestClient(app).post(f"/sync/{conn_id}", headers=BOOK_B)
        assert response.status_code == 404
        # Book A can
        response = TestClient(app).post(f"/sync/{conn_id}", headers=BOOK_A)
        assert response.status_code == 200

    def test_cross_book_transactions_blocked(self):
        r = TestClient(app).post(
            "/connect", params={"bank_name": "CBZ", "account_number": "444", "api_key": "k"}, headers=BOOK_A
        )
        conn_id = r.json()["id"]
        # Book B cannot list Book A's transactions
        response = TestClient(app).get(f"/transactions/{conn_id}", headers=BOOK_B)
        assert response.status_code == 404
        # Book A can (empty list is fine)
        response = TestClient(app).get(f"/transactions/{conn_id}", headers=BOOK_A)
        assert response.status_code == 200


class TestReconciliation:
    def test_reconcile_unknown_transaction(self):
        _connect()
        response = client.post("/transactions/whatever/reconcile", json={"transaction_id": "nope", "notes": ""})
        assert response.status_code == 404
