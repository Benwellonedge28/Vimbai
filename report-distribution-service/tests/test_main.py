"""Report distribution: Book-scoped Neo4j persistence.

The lists, subscriptions and delivery logs used to be module-level lists that
every tenant shared and every restart wiped. These tests pin the new contract:
records survive the request, and one Book never sees another Book's records.
"""

import pytest
import report_distribution_store as store
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


def _create_list(client, headers=BOOK_ONE, name="Monthly pack"):
    # recipients/channels are list-typed body fields, not query params: that is
    # the endpoint's long-standing contract, so the body carries them.
    resp = client.post(
        "/lists",
        params={"name": name, "description": "d"},
        json={"recipients": ["a@b.com"], "channels": ["email"]},
        headers=headers,
    )
    assert resp.status_code == 200, resp.text
    return resp.json()


def test_health(client):
    resp = client.get("/health")
    assert resp.status_code == 200
    assert resp.json()["service"] == "report-distribution-service"


def test_distribution_list_round_trips(client):
    created = _create_list(client)
    assert created["name"] == "Monthly pack"
    listed = client.get("/lists", headers=BOOK_ONE).json()
    assert [row["id"] for row in listed] == [created["id"]]
    assert listed[0]["recipients"] == ["a@b.com"]


def test_subscription_requires_a_list_in_the_same_book(client):
    _create_list(client)
    missing = client.post(
        "/subscriptions",
        params={"report_type": "trial_balance", "distribution_list_id": "nope"},
        headers=BOOK_ONE,
    )
    assert missing.status_code == 404


def test_distribution_writes_a_log(client):
    created = _create_list(client)
    sub = client.post(
        "/subscriptions",
        params={"report_type": "trial_balance", "distribution_list_id": created["id"], "frequency": "monthly"},
        headers=BOOK_ONE,
    ).json()
    log = client.post(f"/subscriptions/{sub['id']}/distribute", headers=BOOK_ONE)
    assert log.status_code == 200, log.text
    assert log.json()["status"] == "sent"
    assert log.json()["recipients"] == ["a@b.com"]
    logs = client.get("/logs", headers=BOOK_ONE).json()
    assert len(logs) == 1


def test_one_book_never_sees_another_books_records(client):
    mine = _create_list(client, BOOK_ONE, name="Mine")
    assert client.get("/lists", headers=BOOK_TWO).json() == []
    other_sub = client.post(
        "/subscriptions",
        params={"report_type": "x", "distribution_list_id": mine["id"]},
        headers=BOOK_TWO,
    )
    assert other_sub.status_code == 404
    stolen = client.post(f"/subscriptions/{mine['id']}/distribute", headers=BOOK_TWO)
    assert stolen.status_code == 404


def test_invalid_frequency_is_rejected(client):
    created = _create_list(client)
    resp = client.post(
        "/subscriptions",
        params={"report_type": "x", "distribution_list_id": created["id"], "frequency": "hourly"},
        headers=BOOK_ONE,
    )
    assert resp.status_code == 400
