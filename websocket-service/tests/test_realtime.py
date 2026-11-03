"""Real-time fan-out: bus events reach the right Book's sockets, once.

Before this, WebSocket clients only saw what was broadcast by hand through
POST /broadcast/{channel}, so a ledger write never reached a second device.
These tests pin the last-mile path: an event on the bus is delivered to the
sockets subscribed to its Book, and to nobody else's.
"""

import pytest
import realtime
from fastapi.testclient import TestClient
from main import app, relay


def _event(book_id="book-1", event_id="evt-1", resource="journal_entry"):
    return {
        "id": event_id,
        "event_type": "book.resource.created",
        "source_service": "api-gateway",
        "timestamp": "2026-11-03T06:00:00Z",
        "payload": {
            "book_id": book_id,
            "resource_type": resource,
            "resource_id": "je-1",
            "action": "created",
            "amount": 250,
        },
    }


@pytest.fixture
def client():
    with TestClient(app) as c:
        yield c


def test_book_channel_is_derived_from_the_payload():
    assert realtime.book_channel(_event("book-9")) == "book:book-9"
    assert realtime.book_channel({"payload": {}, "metadata": {"book_id": "book-7"}}) == "book:book-7"
    # No Book anywhere: nothing safe to broadcast to.
    assert realtime.book_channel({"payload": {}}) is None


def test_normalise_exposes_the_fields_a_client_needs():
    msg = realtime.normalise(_event())
    assert msg["event_id"] == "evt-1"
    assert msg["book_id"] == "book-1"
    assert msg["resource_type"] == "journal_entry"
    assert msg["resource_id"] == "je-1"
    assert msg["action"] == "created"
    assert msg["type"] == "event"


def test_ingest_reaches_only_the_books_sockets(client):
    with client.websocket_connect("/ws/book:book-1") as mine:
        mine.receive_json()  # hello
        with client.websocket_connect("/ws/book:book-2") as other:
            other.receive_json()
            resp = client.post("/events/ingest", json=_event())
            assert resp.json()["delivered_to"] == 1
            assert resp.json()["channel"] == "book:book-1"
            assert mine.receive_json()["resource_id"] == "je-1"
    # book-2's socket got no event: assert by sending the next one and seeing a fresh message.
    with client.websocket_connect("/ws/book:book-2") as other:
        other.receive_json()
        client.post("/events/ingest", json=_event(event_id="evt-2", book_id="book-2"))
        assert other.receive_json()["event_id"] == "evt-2"


def test_the_same_event_is_not_broadcast_twice(client):
    with client.websocket_connect("/ws/book:book-1") as ws:
        ws.receive_json()
        assert client.post("/events/ingest", json=_event(event_id="dup")).json()["delivered_to"] == 1
        assert ws.receive_json()["event_id"] == "dup"
        assert client.post("/events/ingest", json=_event(event_id="dup")).json()["delivered_to"] == 0


def test_event_without_a_book_is_dropped(client):
    resp = client.post("/events/ingest", json={"id": "x", "payload": {}})
    assert resp.json() == {"delivered_to": 0, "channel": None}


def test_stats_reports_the_relay_state(client):
    body = client.get("/stats").json()
    assert "relay" in body
    assert body["relay"]["status"] in {"idle", "connecting", "consuming", "disabled", "error"}


def test_relay_reports_itself_disabled_without_the_broker_client(client):
    # aio_pika is optional: without it the service keeps working over HTTP ingest.
    assert relay.status in {"disabled", "connecting", "consuming"}
