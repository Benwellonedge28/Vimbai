"""Book-scoping, persistence and behaviour tests for activity-based-budget-service (fake Neo4j harness).

Covers: activity creation, budget creation with driver-rate costing
(only the caller's activities resolve), totals, status filter, get-by-id
404 contract, approve upsert, and cross-user / cross-Book isolation.
"""

import importlib.util
import os

import main  # noqa: F401  (isort: keep bare main import first)
import pytest
from activity_based_budget_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("abb_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "abb-user-1", "abb-user-2"
BOOK_A, BOOK_B = "abb-book-a", "abb-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _activity(name="Setups", driver="machine_hours", rate=50.0, headers=H1, **extra):
    p = {
        "name": name,
        "description": "Batch setups",
        "cost_pool": "Manufacturing",
        "driver": driver,
        "driver_rate": rate,
    }
    p.update(extra)
    r = client.post("/activities", params=p, headers=headers)
    assert r.status_code == 200, r.text
    return r.json()


def test_activity_crud_and_persistence():
    a = _activity()
    assert a["driver_rate"] == 50.0
    assert a["id"]

    listed = client.get("/activities", headers=H1).json()
    assert len(listed) == 1
    assert listed[0]["id"] == a["id"]

    # second activity, distinct user
    _activity(name="QC", headers=H2)
    assert len(client.get("/activities", headers=H1).json()) == 1
    assert len(client.get("/activities", headers=H2).json()) == 1


def test_budget_creation_costs_by_driver_rate():
    a1 = _activity(name="Setups", driver="setups", rate=50.0)
    a2 = _activity(name="Inspection", driver="inspections", rate=15.0)

    items = [
        {"activity_id": a1["id"], "expected_driver_volume": 100, "notes": "Q1 setups"},
        {"activity_id": a2["id"], "expected_driver_volume": 40},
    ]
    r = client.post(
        "/budgets",
        params={"name": "FY2027 Factory", "fiscal_year": "FY2027", "period": "2026-10"},
        json=items,
        headers=H1,
    )
    assert r.status_code == 200, r.text
    b = r.json()
    assert len(b["line_items"]) == 2
    assert b["line_items"][0]["budgeted_cost"] == 5000.0
    assert b["line_items"][1]["budgeted_cost"] == 600.0
    assert b["total_budget"] == 5600.0
    assert b["status"] == "draft"
    assert b["line_items"][0]["period"] == "2026-10"

    # persisted and filterable
    listed = client.get("/budgets", headers=H1).json()
    assert len(listed) == 1
    assert listed[0]["id"] == b["id"]

    # unknown activity keeps the 404 contract
    r = client.post(
        "/budgets",
        params={"name": "Bad", "fiscal_year": "FY2027", "period": "2026-10"},
        json=[{"activity_id": "does-not-exist", "expected_driver_volume": 1}],
        headers=H1,
    )
    assert r.status_code == 404


def test_budget_get_filter_and_approve():
    a = _activity()
    r = client.post(
        "/budgets",
        params={"name": "B1", "fiscal_year": "FY2027", "period": "2026-10"},
        json=[{"activity_id": a["id"], "expected_driver_volume": 10}],
        headers=H1,
    )
    bid = r.json()["id"]
    _ = client.post(
        "/budgets",
        params={"name": "B2", "fiscal_year": "FY2027", "period": "2026-11"},
        json=[{"activity_id": a["id"], "expected_driver_volume": 20}],
        headers=H1,
    )

    # approve persists status (single record, not a duplicate)
    r = client.put("/budgets/{}/approve".format(bid), headers=H1)
    assert r.status_code == 200
    assert r.json() == {"budget_id": bid, "status": "approved"}

    approved = client.get("/budgets", params={"status": "approved"}, headers=H1).json()
    assert len(approved) == 1
    assert approved[0]["id"] == bid
    assert len(client.get("/budgets", params={"status": "draft"}, headers=H1).json()) == 1

    # get by id reflects approved status
    got = client.get("/budgets/{}".format(bid), headers=H1).json()
    assert got["status"] == "approved"

    # unknown budget keeps the 404 contract
    assert client.get("/budgets/NOPE", headers=H1).status_code == 404
    assert client.put("/budgets/NOPE/approve", headers=H1).status_code == 404


def test_cross_user_and_book_isolation():
    a1 = _activity(name="U1-Act", headers=H1)
    a2 = _activity(name="U2-Act", headers=H2)
    ab = _activity(name="BookB-Act", headers=HB)

    # each user's budget only resolves their own activities
    r1 = client.post(
        "/budgets",
        params={"name": "U1-B", "fiscal_year": "FY2027", "period": "2026-10"},
        json=[{"activity_id": a1["id"], "expected_driver_volume": 10}],
        headers=H1,
    )
    assert r1.status_code == 200
    r2 = client.post(
        "/budgets",
        params={"name": "U2-B", "fiscal_year": "FY2027", "period": "2026-10"},
        json=[{"activity_id": a2["id"], "expected_driver_volume": 10}],
        headers=H2,
    )
    assert r2.status_code == 200
    rb = client.post(
        "/budgets",
        params={"name": "B-B", "fiscal_year": "FY2027", "period": "2026-10"},
        json=[{"activity_id": ab["id"], "expected_driver_volume": 10}],
        headers=HB,
    )
    assert rb.status_code == 200

    # a foreign activity id does not resolve (cross-user injection 404)
    r = client.post(
        "/budgets",
        params={"name": "Steal", "fiscal_year": "FY2027", "period": "2026-10"},
        json=[{"activity_id": a2["id"], "expected_driver_volume": 10}],
        headers=H1,
    )
    assert r.status_code == 404

    # budgets are isolated per user and Book
    assert len(client.get("/budgets", headers=H1).json()) == 1
    assert len(client.get("/budgets", headers=H2).json()) == 1
    assert len(client.get("/budgets", headers=HB).json()) == 1

    # get/approve by id is caller-scoped
    bid1 = r1.json()["id"]
    assert client.get("/budgets/{}".format(bid1), headers=H2).status_code == 404
    assert client.put("/budgets/{}/approve".format(bid1), headers=H2).status_code == 404
    assert client.put("/budgets/{}/approve".format(bid1), headers=HB).status_code == 404
