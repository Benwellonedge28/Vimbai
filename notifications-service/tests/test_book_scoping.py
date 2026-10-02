"""Book-scoping and persistence tests for notifications-service (fake Neo4j harness).

Covers: send + own-inbox persistence, cross-user inbox read 403,
mark-read/delete ownership, Book isolation on the inbox view,
preferences round-trip, stats scoped to the caller, and the
websocket mark_read no longer scanning other users' inboxes.
"""

import importlib.util
import json
import os

import main
import pytest
from fastapi.testclient import TestClient
from notifications_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ntf_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "ntf-user-1", "ntf-user-2"
BOOK_A, BOOK_B = "ntf-book-a", "ntf-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _notification(recipients, title="Heads up", ntype="system", **kw):
    body = {
        "type": ntype,
        "title": title,
        "message": "test notification body",
        "recipients": recipients,
        "channels": ["in_app"],
    }
    body.update(kw)
    return body


# ---------------------------------------------------------------------------
# Health
# ---------------------------------------------------------------------------


def test_health():
    r = client.get("/health")
    assert r.status_code == 200
    assert r.json()["status"] == "healthy"


def test_identity_header_required():
    assert client.post("/notifications", json=_notification([U1])).status_code == 422


# ---------------------------------------------------------------------------
# Send + inbox
# ---------------------------------------------------------------------------


def test_send_and_own_inbox_persists():
    r = client.post("/notifications", json=_notification([U1]), headers=H1)
    assert r.status_code == 201, r.text
    nid = r.json()["id"]
    assert r.json()["sender"] == U1  # sender is the verified caller now

    fresh = TestClient(app)
    inbox = fresh.get(f"/notifications/{U1}", headers=H1).json()
    assert inbox["total_count"] == 1
    assert inbox["unread_count"] == 1
    assert inbox["notifications"][0]["id"] == nid


def test_inbox_is_caller_only():
    client.post("/notifications", json=_notification([U1]), headers=H1)

    # another user cannot read U1's inbox (previously: anyone could)
    assert client.get(f"/notifications/{U1}", headers=H2).status_code == 403
    # U2's own inbox is empty
    assert client.get(f"/notifications/{U2}", headers=H2).json()["total_count"] == 0


def test_book_isolation_on_inbox():
    # sent under BOOK_A by U1, to U2
    client.post("/notifications", json=_notification([U2]), headers=H1)
    # U2 reading in BOOK_A sees it
    assert client.get(f"/notifications/{U2}", headers=H2).json()["total_count"] == 1
    # U2 reading in BOOK_B does not
    other_book = {"X-User-Id": U2, "X-Book-ID": BOOK_B}
    assert client.get(f"/notifications/{U2}", headers=other_book).json()["total_count"] == 0
    # personal (no Book) view sees everything sent to them
    assert client.get(f"/notifications/{U2}", headers={"X-User-Id": U2}).json()["total_count"] == 1


def test_mark_read_and_delete_ownership():
    nid = client.post("/notifications", json=_notification([U1]), headers=H1).json()["id"]
    # a notification sent to U2, not U1
    nid2 = client.post("/notifications", json=_notification([U2]), headers=H1).json()["id"]

    # U2 cannot mark/delete U1's inbox item
    assert client.put(f"/notifications/{nid}/read", headers=H2).status_code == 404
    assert client.delete(f"/notifications/{nid}", headers=H2).status_code == 404
    # and vice versa
    assert client.put(f"/notifications/{nid2}/read", headers=H1).status_code == 404
    assert client.delete(f"/notifications/{nid2}", headers=H1).status_code == 404

    # owner marks + deletes own
    assert client.put(f"/notifications/{nid}/read", headers=H1).status_code == 200
    inbox = client.get(f"/notifications/{U1}", headers=H1).json()
    assert inbox["unread_count"] == 0
    assert client.delete(f"/notifications/{nid}", headers=H1).status_code == 200
    assert client.get(f"/notifications/{U1}", headers=H1).json()["total_count"] == 0


def test_read_all_scoped_to_caller():
    client.post("/notifications", json=_notification([U1]), headers=H1)
    client.post("/notifications", json=_notification([U2]), headers=H1)

    # U1 cannot mark U2's inbox
    assert client.put(f"/notifications/{U2}/read-all", headers=H1).status_code == 403

    r = client.put(f"/notifications/{U1}/read-all", headers=H1)
    assert r.json()["marked_count"] == 1
    # U2's item untouched
    assert client.get(f"/notifications/{U2}", headers=H2).json()["unread_count"] == 1


def test_batch_send():
    r = client.post(
        "/notifications/batch",
        json=[_notification([U1], title="one"), _notification([U2], title="two")],
        headers=H1,
    )
    assert r.status_code == 201
    assert r.json()["count"] == 2
    assert client.get(f"/notifications/{U1}", headers=H1).json()["total_count"] == 1


def test_stats_scoped_to_caller():
    client.post("/notifications", json=_notification([U1]), headers=H1)
    client.post("/notifications", json=_notification([U2]), headers=H1)

    s1 = client.get("/stats", headers=H1).json()
    assert s1["total_notifications"] == 1  # previously counted ALL users
    assert s1["unread_notifications"] == 1
    assert client.get("/stats", headers=H2).json()["total_notifications"] == 1


# ---------------------------------------------------------------------------
# Preferences (previously a no-op PUT + hardcoded GET)
# ---------------------------------------------------------------------------


def test_preferences_roundtrip_and_gating():
    # defaults when never set
    prefs = client.get(f"/preferences/{U1}", headers=H1).json()
    assert prefs["email_batch"] is True

    saved = client.put(
        f"/preferences/{U1}",
        json={
            "user_id": U1,
            "quiet_hours_start": "22:00",
            "quiet_hours_end": "07:00",
            "email_batch": False,
            "email_batch_interval_minutes": 30,
        },
        headers=H1,
    )
    assert saved.status_code == 200

    # persists on a fresh client
    fresh = TestClient(app).get(f"/preferences/{U1}", headers=H1).json()
    assert fresh["quiet_hours_start"] == "22:00"
    assert fresh["email_batch"] is False
    assert fresh["email_batch_interval_minutes"] == 30

    # another user cannot read or write U1's preferences
    assert client.get(f"/preferences/{U1}", headers=H2).status_code == 403
    assert client.put(f"/preferences/{U1}", json={"user_id": U1}, headers=H2).status_code == 403


# ---------------------------------------------------------------------------
# Websocket (mark_read previously scanned EVERY user's inbox)
# ---------------------------------------------------------------------------


def test_websocket_mark_read_scoped_to_own_inbox():
    nid = client.post("/notifications", json=_notification([U1]), headers=H1).json()["id"]
    nid2 = client.post("/notifications", json=_notification([U2]), headers=H1).json()["id"]

    with client.websocket_connect(f"/ws/notifications/{U1}", headers={"X-User-Id": U1}) as ws:
        # connect pushes this user's unread count
        assert ws.receive_json() == {"type": "unread_count", "count": 1}
        # U1's socket tries to mark U2's notification read
        ws.send_text(json.dumps({"type": "mark_read", "notification_id": nid2}))
        ack = ws.receive_json()
        assert ack["type"] == "notification_read"
        # and its own
        ws.send_text(json.dumps({"type": "mark_read", "notification_id": nid}))
        ws.receive_json()
        # mark all
        ws.send_text(json.dumps({"type": "mark_all_read"}))

    # U2's notification untouched by U1's socket activity
    assert client.get(f"/notifications/{U2}", headers=H2).json()["unread_count"] == 1
    # U1's own is read
    assert client.get(f"/notifications/{U1}", headers=H1).json()["unread_count"] == 0


def test_websocket_receives_new_notification():
    # one unread before connecting so connect pushes a first frame
    client.post("/notifications", json=_notification([U1]), headers=H1)
    with client.websocket_connect(f"/ws/notifications/{U1}", headers={"X-User-Id": U1}) as ws:
        assert ws.receive_json() == {"type": "unread_count", "count": 1}
        client.post("/notifications", json=_notification([U1], title="second"), headers=H1)
        frame = ws.receive_json()
        assert frame["type"] == "notification"
        assert frame["notification"]["title"] == "second"
