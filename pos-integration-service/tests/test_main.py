"""Book-scoping and persistence tests for pos-integration-service (fake Neo4j harness)."""

import importlib.util
import os

import main  # noqa: F401 (bootstraps the pos_integration_service package alias)
import pytest
from fastapi.testclient import TestClient
from pos_integration_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("pos_root_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "pos-user-1", "pos-user-2"
BOOK_A, BOOK_B = "pos-book-a", "pos-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _device_payload(device_id="pos-001", name="Front Counter"):
    return {
        "device_id": device_id,
        "device_name": name,
        "device_type": "clover",
        "location_id": "store-1",
    }


def _transaction_payload(transaction_id="tx-001", device_id="pos-001", amount=50.0, ttype="sale", method="cash"):
    return {
        "transaction_id": transaction_id,
        "device_id": device_id,
        "transaction_type": ttype,
        "total_amount": amount,
        "tax_amount": 5.0,
        "payment_method": method,
        "items": [{"sku": "A1", "qty": 2, "price": 25.0}],
    }


def test_register_device_and_scoping():
    r = client.post("/devices", json=_device_payload(), headers=H1)
    assert r.status_code == 201
    d = r.json()
    assert d["device_id"] == "pos-001"
    assert d["status"] == "offline"

    # duplicate within the caller's own set: 409
    assert client.post("/devices", json=_device_payload(), headers=H1).status_code == 409

    # Book-gated: same caller, other Book sees none
    other_book = dict(H1)
    other_book["X-Book-ID"] = BOOK_B
    assert client.get("/devices", headers=other_book).json() == []

    # cross-caller read: 404 (before U2 registers their own device with the same id)
    assert client.get("/devices/pos-001", headers=H2).status_code == 404
    # owner read ok
    assert client.get("/devices/pos-001", headers=H1).json()["id"] == d["id"]

    # status update: foreign 404, owner ok
    assert client.put("/devices/pos-001/status", json={"status": "online"}, headers=H2).status_code == 404

    # same external device id is fine for a different caller (per-caller registries)
    r2 = client.post("/devices", json=_device_payload(), headers=H2)
    assert r2.status_code == 201
    r = client.put("/devices/pos-001/status", json={"status": "online"}, headers=H1)
    assert r.json() == {"status": "updated", "device_id": "pos-001"}
    # persisted
    got = client.get("/devices/pos-001", headers=H1).json()
    assert got["status"] == "online"
    assert got["last_sync"] is not None
    # status filter
    assert len(client.get("/devices", params={"status": "online"}, headers=H1).json()) == 1
    assert client.get("/devices", params={"status": "offline"}, headers=H1).json() == []


def test_transaction_ingestion_scoping():
    r = client.post("/transactions", json=_transaction_payload(), headers=H1)
    assert r.status_code == 201
    t = r.json()
    assert t["sync_status"] == "pending"
    assert t["created_at"] is not None

    # duplicate external id within caller's set: 409
    assert client.post("/transactions", json=_transaction_payload(), headers=H1).status_code == 409

    # cross-caller read: 404 (before U2 ingests their own copy of the same id)
    assert client.get("/transactions/tx-001", headers=H2).status_code == 404
    got = client.get("/transactions/tx-001", headers=H1).json()
    assert got["id"] == t["id"]
    assert got["tax_amount"] == 5.0
    assert got["items"][0]["sku"] == "A1"

    # Book-gated listing
    other_book = dict(H1)
    other_book["X-Book-ID"] = BOOK_B
    assert client.get("/transactions", headers=other_book).json() == []
    assert len(client.get("/transactions", headers=H1).json()) == 1

    # same external id fine for another caller (their own ledger)
    assert client.post("/transactions", json=_transaction_payload(), headers=H2).status_code == 201

    # filters: device_id
    client.post("/transactions", json=_transaction_payload("tx-002", device_id="pos-002"), headers=H1)
    assert len(client.get("/transactions", params={"device_id": "pos-001"}, headers=H1).json()) == 1
    assert len(client.get("/transactions", headers=H1).json()) == 2


def test_batch_ingestion():
    payload = [
        _transaction_payload("b-1"),
        _transaction_payload("b-2", device_id="pos-002"),
        _transaction_payload("b-1"),  # duplicate inside the batch itself
    ]
    r = client.post("/transactions/batch", json=payload, headers=H1)
    assert r.status_code == 201
    body = r.json()
    assert body["total"] == 3
    assert body["results"][0]["status"] == "accepted"
    assert body["results"][1]["status"] == "accepted"
    assert body["results"][2]["status"] == "rejected"
    assert "already received" in body["results"][2]["reason"]


def test_sales_summary_is_caller_scoped():
    client.post("/transactions", json=_transaction_payload("s-1", amount=100.0), headers=H1)
    client.post(
        "/transactions",
        json=_transaction_payload("s-2", amount=30.0, ttype="refund", device_id="pos-001"),
        headers=H1,
    )
    # foreign caller's transactions don't leak into the summary
    client.post("/transactions", json=_transaction_payload("s-9", amount=999.0), headers=H2)

    req = {
        "device_id": "pos-001",
        "start_date": "2020-01-01T00:00:00Z",
        "end_date": "2100-01-01T00:00:00Z",
        "group_by": "day",
    }
    body = client.post("/reports/sales-summary", json=req, headers=H1).json()
    assert body["total_transactions"] == 2
    assert body["total_sales"] == 100.0
    assert body["total_refunds"] == 30.0
    assert body["net_sales"] == 70.0
    assert body["by_payment_method"]["cash"] == 130.0
    assert body["by_type"] == {"sale": 1, "refund": 1, "void": 0}

    # the foreign caller's summary only counts their own data (s-9)
    body2 = client.post("/reports/sales-summary", json=req, headers=H2).json()
    assert body2["total_transactions"] == 1
    assert body2["total_sales"] == 999.0


def test_metrics_and_health_scoped():
    client.post("/devices", json=_device_payload(), headers=H1)
    client.post("/transactions", json=_transaction_payload("m-1", amount=25.0), headers=H1)
    client.post("/transactions", json=_transaction_payload("m-2", amount=15.0), headers=H2)

    m1 = client.get("/metrics", headers=H1).json()
    m2 = client.get("/metrics", headers=H2).json()
    assert m1["registered_devices"] == 1
    assert m1["total_transactions"] == 1
    assert m1["total_amount_processed"] == 25.0
    assert m2["registered_devices"] == 0
    assert m2["total_transactions"] == 1
    assert m2["total_amount_processed"] == 15.0

    h = client.get("/", headers=H1).json()
    assert h["service"] == "pos-integration"
    assert h["total_transactions"] == 1


def test_webhook_ingestion():
    payload = {"id": "sq-1", "location_id": "loc-1", "total_money": {"amount": 2500}, "tax_money": {"amount": 250}}
    r = client.post("/integrations/square/webhook", json=payload, headers=H1)
    assert r.json() == {"status": "received", "transaction_id": "sq-1"}
    t = client.get("/transactions/sq-1", headers=H1).json()
    assert t["total_amount"] == 25.0
    assert t["tax_amount"] == 2.5
    assert t["device_id"] == "loc-1"
    # duplicate webhook for the same caller: 409
    assert client.post("/integrations/square/webhook", json=payload, headers=H1).status_code == 409
    # but fine for another caller
    assert client.post("/integrations/square/webhook", json=payload, headers=H2).status_code == 200


def test_inventory_sync_endpoints():
    r = client.post(
        "/inventory/sync",
        json={"device_id": "pos-001", "products": [{"sku": "A1", "qty": 3}]},
        headers=H1,
    )
    assert r.json()["status"] == "synced"
    assert r.json()["items_updated"] == 1

    r = client.post(
        "/inventory/reconcile",
        params={"device_id": "pos-001"},
        json=[{"sku": "A1", "expected": 5, "actual": 4}],
        headers=H1,
    )
    assert r.json()["device_id"] == "pos-001"
    assert r.json()["total_items"] == 1
