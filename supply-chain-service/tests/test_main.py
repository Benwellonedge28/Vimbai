"""Book-scoping and persistence tests for supply-chain-service (fake Neo4j harness).

Covers: supplier/inventory/PO persistence, low-stock logic, PO receive
semantics (status + stock update), demand forecasting with reorder
recommendation, ownership and Book isolation.
"""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from supply_chain_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("sc_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "sc-user-1", "sc-user-2"
BOOK_A, BOOK_B = "sc-book-a", "sc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _supplier(name="Acme Corp", **kw):
    p = {"name": name, "contact": "acme@example.com", "lead_time_days": 7, "rating": 4.5, "products": ["widgets"]}
    p.update(kw)
    return p


def _item(sku="WIDGET-001", qty=5, company="co-sc", **kw):
    p = {
        "sku": sku,
        "name": "Widget",
        "company_id": company,
        "quantity": qty,
        "reorder_point": 10,
        "reorder_qty": 50,
    }
    p.update(kw)
    return p


def _po(sku="WIDGET-001", company="co-sc", qty=20, cost=2.5, **kw):
    p = {"company_id": company, "supplier_id": "sup-1", "item_sku": sku, "quantity": qty, "unit_cost": cost}
    p.update(kw)
    return p


def test_suppliers_persist_and_scope():
    resp = client.post("/suppliers", json=_supplier(), headers=H1)
    assert resp.status_code == 200, resp.text
    created = resp.json()
    assert created["name"] == "Acme Corp"
    assert created["products"] == ["widgets"]

    mine = client.get("/suppliers", headers=H1).json()
    assert [s["id"] for s in mine] == [created["id"]]
    # other user sees nothing
    assert client.get("/suppliers", headers=H2).json() == []


def test_inventory_low_stock_and_receive():
    client.post("/inventory", json=_item(qty=0), headers=H1)
    client.post("/inventory", json=_item(sku="FULL-001", qty=100), headers=H1)
    inv = client.get("/inventory", params={"company_id": "co-sc"}, headers=H1).json()
    assert {i["sku"] for i in inv} == {"WIDGET-001", "FULL-001"}

    low = client.get("/inventory/low-stock", params={"company_id": "co-sc"}, headers=H1).json()
    assert len(low) == 1
    assert low[0]["sku"] == "WIDGET-001"
    assert low[0]["urgency"] == "critical"
    # qty=5 <= reorder_point=10 -> warning
    client.post("/inventory", json=_item(sku="LOW-001", qty=5), headers=H1)
    low = client.get("/inventory/low-stock", params={"company_id": "co-sc"}, headers=H1).json()
    urgencies = {e["sku"]: e["urgency"] for e in low}
    assert urgencies == {"WIDGET-001": "critical", "LOW-001": "warning"}

    # PO receive updates stock + persists
    po = client.post("/purchase-orders", json=_po(qty=20), headers=H1).json()
    recv = client.post(f"/purchase-orders/{po['id']}/receive", params={"company_id": "co-sc"}, headers=H1)
    assert recv.status_code == 200, recv.text
    assert recv.json() == {"po_id": po["id"], "status": "received", "quantity_added": 20}
    stored = client.get("/inventory", params={"company_id": "co-sc"}, headers=H1).json()
    w = next(i for i in stored if i["sku"] == "WIDGET-001")
    assert w["quantity"] == 20
    pos = client.get("/purchase-orders", params={"company_id": "co-sc"}, headers=H1).json()
    assert pos[0]["status"] == "received"
    # status filter
    assert client.get("/purchase-orders", params={"company_id": "co-sc", "status": "pending"}, headers=H1).json() == []


def test_receive_po_scoping():
    po = client.post("/purchase-orders", json=_po(), headers=H1).json()
    # cross-user, cross-Book, wrong-company all 404
    assert (
        client.post(f"/purchase-orders/{po['id']}/receive", params={"company_id": "co-sc"}, headers=H2).status_code
        == 404
    )
    assert (
        client.post(
            f"/purchase-orders/{po['id']}/receive",
            params={"company_id": "co-sc"},
            headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
        ).status_code
        == 404
    )
    assert (
        client.post(f"/purchase-orders/{po['id']}/receive", params={"company_id": "other-co"}, headers=H1).status_code
        == 404
    )
    # still pending
    pos = client.get("/purchase-orders", params={"company_id": "co-sc"}, headers=H1).json()
    assert pos[0]["status"] == "pending"


def test_forecast_semantics():
    # insufficient data
    r = client.post(
        "/forecast",
        json={"sku": "PROD-001", "company_id": "co-sc", "historical_data": [10], "forecast_periods": 3},
        headers=H1,
    ).json()
    assert r["method"] == "insufficient_data"
    assert r["forecast"] == [0, 0, 0]
    assert r["reorder_recommended"] is False

    # moving average with trend, reorder recommended when projected stock <= reorder point
    client.post("/inventory", json=_item(sku="PROD-001", qty=15), headers=H1)
    r = client.post(
        "/forecast",
        json={
            "sku": "PROD-001",
            "company_id": "co-sc",
            "historical_data": [100, 110, 105, 120, 115],
            "forecast_periods": 3,
        },
        headers=H1,
    ).json()
    assert len(r["forecast"]) == 3
    assert r["method"] == "moving_average_with_trend"
    assert r["reorder_recommended"] is True
    assert r["recommended_qty"] == 50
    # other user: no inventory visible -> no reorder recommendation
    r2 = client.post(
        "/forecast",
        json={
            "sku": "PROD-001",
            "company_id": "co-sc",
            "historical_data": [100, 110, 105, 120, 115],
            "forecast_periods": 3,
        },
        headers=H2,
    ).json()
    assert r2["reorder_recommended"] is False


def test_book_a_b_isolation():
    client.post("/inventory", json=_item(sku="A-001", company="co-sc"), headers=H1)
    hb = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    client.post("/inventory", json=_item(sku="B-001", company="co-sc"), headers=hb)
    assert {i["sku"] for i in client.get("/inventory", params={"company_id": "co-sc"}, headers=H1).json()} == {"A-001"}
    assert {i["sku"] for i in client.get("/inventory", params={"company_id": "co-sc"}, headers=hb).json()} == {"B-001"}
    # personal view spans both Books
    assert len(client.get("/inventory", params={"company_id": "co-sc"}, headers=H1_PERSONAL).json()) == 2


def test_x_user_id_required():
    assert client.post("/suppliers", json=_supplier()).status_code in (401, 403, 422)
    assert client.get("/inventory", params={"company_id": "co-sc"}).status_code in (401, 403, 422)
    assert client.get("/suppliers").status_code in (401, 403, 422)
