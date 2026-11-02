"""Book-scoping, persistence and arithmetic tests for prime-cost-service.

Covers prime cost from stated figures, the same figure derived from recorded
direct cost items, per-item cost per unit, listing filters, and cross-user /
cross-Book isolation now that costs and calculations live in Neo4j.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from fastapi.testclient import TestClient
from prime_costing_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("pc_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "pc-user-1", "pc-user-2"
BOOK_A, BOOK_B = "pc-book-a", "pc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _add_item(name, item_type, amount, units=1, product_id="P-1", headers=H1):
    return client.post(
        "/direct-costs/add",
        params={
            "item_name": name,
            "item_type": item_type,
            "amount": amount,
            "units": units,
            "product_id": product_id,
        },
        headers=headers,
    ).json()


def test_prime_cost_from_stated_figures():
    body = client.post(
        "/calculate",
        params={
            "product_id": "P-1",
            "period": "2026-11",
            "direct_materials": 1000,
            "direct_labor": 400,
            "direct_expenses": 100,
        },
        headers=H1,
    ).json()

    assert body["prime_cost"] == 1500.0
    assert body["cost_breakdown"] == {
        "direct_materials": 1000.0,
        "direct_labor": 400.0,
        "direct_expenses": 100.0,
        "prime_cost": 1500.0,
    }


def test_direct_cost_item_records_cost_per_unit():
    item = _add_item("Steel", "direct_material", 600.0, units=4)
    assert item["cost_per_unit"] == 150.0

    zero = _add_item("Lump sum", "direct_expense", 300.0, units=0)
    assert zero["cost_per_unit"] == 0


def test_prime_cost_derived_from_recorded_items():
    _add_item("Steel", "direct_material", 1000.0)
    _add_item("Fitter", "direct_labor", 400.0)
    _add_item("Freight", "direct_expense", 100.0)
    _add_item("Other product steel", "direct_material", 999.0, product_id="P-2")

    body = client.post("/calculate-with-items", params={"product_id": "P-1", "period": "2026-11"}, headers=H1).json()

    assert body["direct_materials"] == 1000.0
    assert body["direct_labor"] == 400.0
    assert body["direct_expenses"] == 100.0
    assert body["prime_cost"] == 1500.0


def test_derived_prime_cost_ignores_unrecorded_items():
    _add_item("Steel", "direct_material", 500.0, product_id="P-2")

    body = client.post("/calculate-with-items", params={"product_id": "P-1", "period": "2026-11"}, headers=H1).json()
    assert body["prime_cost"] == 0.0


def test_items_and_calculations_persist_and_filter():
    _add_item("Steel", "direct_material", 100.0)
    _add_item("Brass", "direct_material", 200.0, product_id="P-2")
    client.post(
        "/calculate",
        params={"product_id": "P-1", "period": "2026-11", "direct_materials": 100, "direct_labor": 50},
        headers=H1,
    )

    items = client.get("/direct-costs", headers=H1).json()["items"]
    assert len(items) == 2
    mine = client.get("/direct-costs", params={"product_id": "P-1"}, headers=H1).json()["items"]
    assert [i["item_name"] for i in mine] == ["Steel"]

    calcs = client.get("/calculations", headers=H1).json()["calculations"]
    assert len(calcs) == 1
    assert client.get("/calculations", params={"product_id": "P-9"}, headers=H1).json()["calculations"] == []


def test_records_are_caller_and_book_scoped():
    _add_item("Steel", "direct_material", 100.0)
    client.post(
        "/calculate",
        params={"product_id": "P-1", "period": "2026-11", "direct_materials": 100, "direct_labor": 50},
        headers=H1,
    )

    assert client.get("/direct-costs", headers=H2).json()["items"] == []
    assert client.get("/calculations", headers=H2).json()["calculations"] == []
    assert client.get("/direct-costs", headers=HB).json()["items"] == []


def test_from_items_never_reads_another_tenants_costs():
    _add_item("Their steel", "direct_material", 900.0, headers=H2)

    body = client.post("/calculate-with-items", params={"product_id": "P-1", "period": "2026-11"}, headers=H1).json()
    assert body["prime_cost"] == 0.0


def test_caller_header_is_required_for_listings():
    assert client.get("/direct-costs").status_code == 422
    assert client.get("/calculations").status_code == 422


def test_health_reports_the_engine():
    body = client.get("/health").json()
    assert body["service"] == "prime-cost-service"
    assert body["calculation_engine"] in ("rust", "python")
