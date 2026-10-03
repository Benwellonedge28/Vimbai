"""Book-scoping and persistence tests for message-bus-service (fake Neo4j harness)."""

import importlib.util
import os

import main  # noqa: F401 (bootstraps the message_bus_service package alias)
import pytest
from fastapi.testclient import TestClient
from message_bus_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("mb_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "mb-user-1", "mb-user-2"
BOOK_A, BOOK_B = "mb-book-a", "mb-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def test_health():
    body = client.get("/", headers=H1).json()
    assert body["status"] == "healthy"
    assert body["service"] == "message-bus"
    assert body["events_stored"] == 0
    assert body["subscriptions"] == 0


def test_publish_and_event_isolation():
    r = client.post(
        "/trigger/journal-entry-created",
        params={"entry_id": "je-1", "amount": 1200.0},
        headers=H1,
    )
    assert r.status_code == 200, r.text
    eid = r.json()["event_id"]

    client.post(
        "/trigger/budget-variance-alert",
        params={"budget_id": "b-1", "variance_percent": 35.0, "category": "travel"},
        headers=H2,
    )

    # caller sees only own events
    evts = client.get("/events", headers=H1).json()
    assert len(evts) == 1
    assert evts[0]["payload"]["entry_id"] == "je-1"
    assert client.get("/events", headers=H2).json()[0]["type"] == "finance.budget.variance_alert"
    assert client.get("/events", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json() == []

    # direct fetch: own ok, foreign 404
    assert client.get(f"/events/{eid}", headers=H1).json()["id"] == eid
    assert client.get(f"/events/{eid}", headers=H2).status_code == 404

    # filters work over caller's own set
    evts = client.get("/events", params={"event_type": "accounting.journal_entry.created"}, headers=H1).json()
    assert len(evts) == 1
    assert client.get("/events", params={"event_type": "finance.budget.variance_alert"}, headers=H1).json() == []


def test_subscription_lifecycle_and_isolation():
    sub = {
        "id": "sub-1",
        "name": "Ops webhook",
        "event_types": ["accounting.journal_entry.created", "fraud.transaction.flagged"],
        "callback_url": "https://ops.example.com/hook",
        "filter_expression": None,
        "enabled": True,
        "priority": "normal",
    }
    r = client.post("/subscriptions", json=sub, headers=H1)
    assert r.status_code == 201, r.text
    assert r.json()["callback_url"] == "https://ops.example.com/hook"
    assert len(r.json()["event_types"]) == 2

    # foreign read/delete: 404
    assert client.get("/subscriptions/sub-1", headers=H2).status_code == 404
    assert client.delete("/subscriptions/sub-1", headers=H2).status_code == 404

    # same id from another caller does not clobber U1's
    sub2 = dict(sub, id="sub-1", name="Foreign webhook", callback_url="https://evil.example.com/x")
    client.post("/subscriptions", json=sub2, headers=H2)
    assert client.get("/subscriptions/sub-1", headers=H1).json()["name"] == "Ops webhook"

    # owner re-create overwrites own (dict-key semantics)
    sub3 = dict(sub, name="Ops webhook v2")
    r = client.post("/subscriptions", json=sub3, headers=H1)
    assert r.status_code == 201
    listed = client.get("/subscriptions", headers=H1).json()
    assert len(listed) == 1
    assert listed[0]["name"] == "Ops webhook v2"

    # owner delete
    assert client.delete("/subscriptions/sub-1", headers=H1).json()["status"] == "deleted"
    assert client.get("/subscriptions/sub-1", headers=H1).status_code == 404


def test_metrics_and_event_types():
    assert len(client.get("/event-types").json()) > 10

    client.post("/trigger/journal-entry-created", params={"entry_id": "a", "amount": 1.0}, headers=H1)
    client.post(
        "/trigger/transaction-flagged",
        params={"transaction_id": "t-9", "fraud_score": 0.95, "reason": "velocity"},
        headers=H1,
    )
    client.post("/trigger/journal-entry-created", params={"entry_id": "b", "amount": 2.0}, headers=H2)

    m1 = client.get("/metrics", headers=H1).json()
    assert m1["total_events"] == 2
    assert m1["event_counts_by_type"]["accounting.journal_entry.created"] == 1
    assert m1["event_counts_by_type"]["fraud.transaction.flagged"] == 1

    m2 = client.get("/metrics", headers=H2).json()
    assert m2["total_events"] == 1
    assert m2["event_counts_by_type"] == {"accounting.journal_entry.created": 1}


def test_publish_endpoint_body():
    r = client.post(
        "/events/publish",
        json={
            "type": "system.feature.toggled",
            "source_service": "admin-service",
            "payload": {"feature": "dark_mode"},
        },
        headers=H1,
    )
    assert r.status_code == 200, r.text
    assert r.json()["status"] == "published"
    assert r.json()["event_type"] == "system.feature.toggled"

    # stored and scoped
    evts = client.get("/events", headers=H1).json()
    assert evts[0]["payload"] == {"feature": "dark_mode"}
    assert client.get("/events", headers=H2).json() == []
