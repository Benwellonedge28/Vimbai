"""Book-scoping and persistence tests for bank-feed-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from bank_feed_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("bankfeed_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "bankfeed-user-1", "bankfeed-user-2"
BOOK_A, BOOK_B = "bankfeed-book-a", "bankfeed-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _conn_payload(provider="manual", org="org-1", **kw):
    payload = {
        "provider": provider,
        "account_name": "ZB Business Checking",
        "account_type": "checking",
        "account_number_last4": "4242",
        "auto_sync_enabled": True,
        "sync_interval_minutes": 60,
    }
    payload.update(kw)
    return payload


def _tx_payload(conn_id, external_id="ext-1", day=1, **kw):
    payload = {
        "bank_connection_id": conn_id,
        "external_id": external_id,
        "date": f"2026-10-{day:02d}T00:00:00+00:00",
        "amount": 120.50,
        "currency": "USD",
        "description": "POS purchase",
        "merchant_name": "OK Mart",
        "category": "groceries",
        "transaction_type": "debit",
        "pending": False,
    }
    payload.update(kw)
    return payload


def _create_conn(headers=H1, **kw):
    resp = client.post("/connections", json=_conn_payload(**kw), params={"organization_id": "org-1"}, headers=headers)
    assert resp.status_code == 201, resp.text
    return resp.json()


def _rule_payload(**kw):
    payload = {
        "id": "placeholder-id",
        "name": "Groceries auto-match",
        "match_conditions": {"categories": ["groceries"], "amount_max": 500},
        "priority": 10,
        "auto_match_enabled": True,
        "create_journal_entry": False,
        "active": True,
    }
    payload.update(kw)
    return payload


# --- connections ---


def test_connection_crud_and_scoping():
    conn = _create_conn()
    assert conn["organization_id"] == "org-1"
    assert conn["book_id"] == BOOK_A

    listed = client.get("/connections", params={"organization_id": "org-1"}, headers=H1).json()
    assert listed["total"] == 1

    # other organization: empty
    assert client.get("/connections", params={"organization_id": "org-9"}, headers=H1).json()["total"] == 0

    # other user: invisible
    assert client.get(f"/connections/{conn['id']}", headers=H2).status_code == 404
    assert client.get("/connections", params={"organization_id": "org-1"}, headers=H2).json()["total"] == 0

    # other book: invisible
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get(f"/connections/{conn['id']}", headers=other).status_code == 404

    # delete: caller-owned
    resp = client.delete(f"/connections/{conn['id']}", headers=H1)
    assert resp.status_code == 204
    assert client.get(f"/connections/{conn['id']}", headers=H1).status_code == 404
    # cross-user delete: 404
    conn2 = _create_conn()
    assert client.delete(f"/connections/{conn2['id']}", headers=H2).status_code == 404


# --- transactions ---


def test_import_duplicate_link_status():
    conn = _create_conn()

    resp = client.post("/transactions/import", json=_tx_payload(conn["id"]), headers=H1)
    assert resp.status_code == 201, resp.text
    tx = resp.json()
    assert tx["status"] == "cleared"
    assert tx["book_id"] == BOOK_A

    # duplicate: 409 with existing id header
    resp = client.post("/transactions/import", json=_tx_payload(conn["id"]), headers=H1)
    assert resp.status_code == 409
    assert resp.headers.get("X-Existing-Transaction-ID") == tx["id"]

    # other user cannot import against this connection
    assert (
        client.post("/transactions/import", json=_tx_payload(conn["id"], external_id="ext-2"), headers=H2).status_code
        == 404
    )

    # import into unknown connection: 404
    assert client.post("/transactions/import", json=_tx_payload("nope"), headers=H1).status_code == 404

    # link persists + reconciles
    resp = client.post(f"/transactions/{tx['id']}/link", params={"journal_entry_id": "je-1"}, headers=H1)
    assert resp.status_code == 200
    linked = resp.json()
    assert linked["linked_journal_entry_id"] == "je-1"
    assert linked["status"] == "reconciled"
    assert client.get(f"/transactions/{tx['id']}", headers=H1).json()["status"] == "reconciled"

    # status update with notes persists
    resp = client.put(
        f"/transactions/{tx['id']}/status",
        params={"status": "disputed", "notes": "called the bank"},
        headers=H1,
    )
    assert resp.status_code == 200
    got = client.get(f"/transactions/{tx['id']}", headers=H1).json()
    assert got["status"] == "disputed"
    assert got["metadata"]["status_notes"] == "called the bank"

    # cross-user status change: 404
    assert client.put(f"/transactions/{tx['id']}/status", params={"status": "cleared"}, headers=H2).status_code == 404


def test_batch_import_and_listing_filters():
    conn = _create_conn()
    batch = [_tx_payload(conn["id"], external_id=f"b-{i}", day=i + 1, amount=10 + i) for i in range(3)]
    batch.append(_tx_payload(conn["id"], external_id="b-1"))  # duplicate inside the batch
    resp = client.post("/transactions/import-batch", json=batch, headers=H1)
    assert resp.status_code == 201
    data = resp.json()
    assert data["imported"] == 3
    assert data["duplicates"] == 1
    assert data["failed"] == 0

    listed = client.get("/transactions", headers=H1).json()
    assert listed["total"] == 3
    assert listed["transactions"][0]["amount"] >= listed["transactions"][-1]["amount"]  # newest first

    # status filter
    assert client.get("/transactions", params={"status": "cleared"}, headers=H1).json()["total"] == 3
    assert client.get("/transactions", params={"status": "pending"}, headers=H1).json()["total"] == 0

    # other user sees nothing
    assert client.get("/transactions", headers=H2).json()["total"] == 0


def test_mt940_import():
    conn = _create_conn()
    statement = (
        ":61:2610011001C500,00\n"
        ":82:REF100\n"
        ":86:Lunch deposit\n"
        ":61:2610011001D120,50\n"
        ":82:REF101\n"
        ":86:Fuel purchase\n"
    )
    resp = client.post(
        "/transactions/import-mt940", params={"connection_id": conn["id"], "statement_data": statement}, headers=H1
    )
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["parsed"] == 2
    assert data["imported"] == 2

    # other user: 404 on the connection
    resp = client.post(
        "/transactions/import-mt940", params={"connection_id": conn["id"], "statement_data": statement}, headers=H2
    )
    assert resp.status_code == 404


# --- reconciliation rules ---


def test_rules_crud_and_auto_match():
    conn = _create_conn()
    rule = client.post("/rules", json=_rule_payload(), headers=H1).json()
    assert rule["book_id"] == BOOK_A

    # auto-match applies on import
    tx = client.post("/transactions/import", json=_tx_payload(conn["id"]), headers=H1).json()
    assert tx["matched_rule_id"] == rule["id"]
    assert tx["confidence_score"] == 0.85

    # a transaction that doesn't match (wrong category) stays unmatched
    tx2 = client.post(
        "/transactions/import", json=_tx_payload(conn["id"], external_id="ext-x", category="travel"), headers=H1
    ).json()
    assert tx2["matched_rule_id"] is None

    # rules are caller-scoped: other user lists none, can't touch ours
    assert client.get("/rules", headers=H2).json()["total"] == 0
    assert client.put(f"/rules/{rule['id']}", json=_rule_payload(name="hijack"), headers=H2).status_code == 404
    assert client.delete(f"/rules/{rule['id']}", headers=H2).status_code == 404

    # update + deactivate persist
    upd = client.put(f"/rules/{rule['id']}", json=_rule_payload(active=False), headers=H1).json()
    assert upd["active"] is False
    assert client.get("/rules", params={"active_only": True}, headers=H1).json()["total"] == 0

    # manual apply endpoint only processes caller's transactions
    apply_resp = client.post("/rules/apply", json=[tx["id"]], headers=H1).json()
    assert apply_resp["processed"] == 1

    # delete works
    assert client.delete(f"/rules/{rule['id']}", headers=H1).status_code == 204
    assert client.get("/rules", headers=H1).json()["total"] == 0


# --- sync + statistics ---


def test_sync_and_statistics_scoped():
    conn = _create_conn()
    resp = client.post("/sync/start", json={"bank_connection_id": conn["id"]}, headers=H1)
    assert resp.status_code == 200, resp.text
    sync_id = resp.json()["sync_id"]

    # sync record is queryable (caller-owned); TestClient runs the background
    # task before this assertion, so the manual-provider sync is completed
    got = client.get(f"/sync/{sync_id}", headers=H1).json()
    assert got["sync_id"] == sync_id
    assert got["status"] == "completed"
    assert client.get(f"/sync/{sync_id}", headers=H2).status_code == 404

    # connection reflects the finished sync
    c = client.get(f"/connections/{conn['id']}", headers=H1).json()
    assert c["last_sync_status"] == "completed"
    assert c["last_sync_at"] is not None

    hist = client.get("/sync/history", headers=H1).json()
    assert hist["total"] == 1

    # statistics scoped to caller's org connections
    stats = client.get("/statistics", params={"organization_id": "org-1"}, headers=H1).json()
    assert stats["total_connections"] == 1
    assert stats["total_transactions"] == 0
    assert stats["by_provider"]["manual"]["connections"] == 1

    # other user's stats: empty
    assert client.get("/statistics", params={"organization_id": "org-1"}, headers=H2).json()["total_connections"] == 0


def test_webhook_requires_owner_context():
    conn = _create_conn()
    payload = {
        "webhook_type": "TRANSACTIONS",
        "connection_id": conn["id"],
        "transactions": [
            {"transaction_id": "pl-1", "date": "2026-10-01T00:00:00+00:00", "amount": 5.0, "name": "Coffee"}
        ],
    }
    resp = client.post("/webhooks/plaid", json=payload, headers=H1)
    assert resp.status_code == 200
    # imported under the webhook-routed owner's context
    assert client.get("/transactions", headers=H1).json()["total"] == 1
    # unknown provider: 400
    assert client.post("/webhooks/nonsense", json={}, headers=H1).status_code == 400


def test_health():
    resp = client.get("/")
    assert resp.status_code == 200
    assert resp.json()["status"] == "healthy"
