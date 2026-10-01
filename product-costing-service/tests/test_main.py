"""Book-scoping and persistence tests for product-costing-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from product_costing_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("product_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "product-user-1", "product-user-2"
BOOK_A, BOOK_B = "product-book-a", "product-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _payload(company="co-product", **kw):
    payload = {
        "company_id": company,
        "product_or_process": "Widget Assembly",
        "period": "2026-Q1",
        "quantity": 100,
        "components": [
            {"name": "Raw materials", "amount": 5000, "cost_type": "direct_materials"},
            {"name": "Labour", "amount": 3000, "cost_type": "direct_labor"},
            {"name": "Overhead", "amount": 2000, "cost_type": "overhead"},
        ],
    }
    payload.update(kw)
    return payload


def test_calculate_and_list():
    resp = client.post("/calculate", json=_payload(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["total_cost"] == 10000.0
    assert data["unit_cost"] == 100.0
    assert data["book_id"] == BOOK_A

    listed = client.get("/calculations/co-product", headers=H1).json()
    assert listed["total"] == 1

    # product filter (case-insensitive substring)
    assert client.get("/calculations/co-product", params={"product": "widget"}, headers=H1).json()["total"] == 1
    assert client.get("/calculations/co-product", params={"product": "zzz"}, headers=H1).json()["total"] == 0

    # other user sees nothing
    assert client.get("/calculations/co-product", headers=H2).json()["total"] == 0


def test_breakdown_persists_and_scopes():
    calc_id = client.post("/calculate", json=_payload(), headers=H1).json()["id"]
    resp = client.get(f"/breakdown/co-product/{calc_id}", headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["total"] == 10000.0
    assert data["unit_cost"] == 100.0
    assert data["breakdown"] == {"direct_materials": 5000.0, "direct_labor": 3000.0, "overhead": 2000.0}

    # cross-user and cross-Book breakdown 404
    assert client.get(f"/breakdown/co-product/{calc_id}", headers=H2).status_code == 404
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get(f"/breakdown/co-product/{calc_id}", headers=other_book).status_code == 404


def test_summary_scoped():
    client.post("/calculate", json=_payload(company="co-sum"), headers=H1)
    client.post("/calculate", json=_payload(company="co-sum", product_or_process="Other"), headers=H1)
    summary = client.get("/summary/co-sum", headers=H1).json()
    assert summary["total_calculations"] == 2
    assert summary["total_cost"] == 20000.0
    assert summary["avg_unit_cost"] == 100.0

    other = client.get("/summary/co-sum", headers=H2).json()
    assert other["total_calculations"] == 0


def test_book_a_b_isolation():
    client.post("/calculate", json=_payload(company="co-a"), headers=H1)
    client.post("/calculate", json=_payload(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/calculations/co-a", headers=H1).json()["total"] == 1
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/calculations/co-a", headers=other_book).json()["total"] == 0
    # personal spans books
    assert client.get("/calculations/co-a", headers=H1_PERSONAL).json()["total"] == 1
    assert client.get("/calculations/co-b", headers=H1_PERSONAL).json()["total"] == 1


def test_zero_quantity_guard():
    data = client.post("/calculate", json=_payload(quantity=0), headers=H1).json()
    assert data["unit_cost"] == data["total_cost"]  # max(1, quantity) guard kept


def test_x_user_id_required():
    assert client.post("/calculate", json=_payload()).status_code in (401, 403, 422)
    assert client.get("/calculations/co-product").status_code in (401, 403, 422)
