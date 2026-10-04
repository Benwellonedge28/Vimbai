"""Book-scoping, persistence and behaviour tests for departmental-accounting-service (fake Neo4j harness).

Covers: department CRUD + hierarchy, allocation rules, cost pools, the
/allocate pipeline with latest-wins results, inter-department billing,
financials/performance/comparison reports, ownership and Book
isolation, and cross-scope 404s. Internal API calls (accounting /
budgeting services) fall back to {} on connection failure, matching the
original tolerance.
"""

import importlib.util
import os

import main  # noqa: F401  (isort: keep bare main import first)
import pytest
from departmental_accounting_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("depta_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "depta-user-1", "depta-user-2"
BOOK_A, BOOK_B = "depta-book-a", "depta-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

P = {"period_start": "2026-10-01T00:00:00Z", "period_end": "2026-10-31T00:00:00Z"}


def _dept(code="ENG", name="Engineering", parent=None, headers=H1, **extra):
    body = {
        "id": f"d-{code}",
        "department_code": code,
        "department_name": name,
        "department_type": "revenue",
        "manager_id": "m1",
        "manager_name": "Manager One",
        "parent_department_id": parent,
    }
    body.update(extra)
    r = client.post("/departments", json=body, headers=headers)
    assert r.status_code == 200, r.text
    return r.json()


def _pool(name="IT Costs", amount="10000.00", excluded=None, headers=H1):
    body = {
        "id": f"p-{name}",
        "pool_name": name,
        "pool_type": "service_costs",
        "total_amount": amount,
        "allocation_basis": "headcount",
        "allocation_method": "direct",
        "included_departments": [],
        "excluded_departments": excluded or [],
        **P,
    }
    r = client.post("/cost-pools", json=body, headers=headers)
    assert r.status_code == 200, r.text
    return r.json()


def test_departments_crud_and_persistence():
    dept = _dept()
    did = dept["id"]

    got = client.get(f"/departments/{did}", headers=H1).json()
    assert got["department_name"] == "Engineering"
    assert got["status"] == "active"

    updated = dict(dept, department_name="Platform Engineering")
    r = client.put(f"/departments/{did}", json=updated, headers=H1)
    assert r.status_code == 200
    assert client.get(f"/departments/{did}", headers=H1).json()["department_name"] == "Platform Engineering"

    # filters
    _dept(code="HR", name="People", headers=H1)
    r = client.get("/departments", params={"department_type": "revenue"}, headers=H1)
    assert len(r.json()) == 2
    r = client.get("/departments", headers=H1)
    assert len(r.json()) == 2


def test_department_hierarchy():
    parent = _dept(code="OPS", name="Operations")
    child = _dept(code="FIN", name="Finance", parent=parent["id"])
    _dept(code="ACC", name="Accounting", parent=child["id"])

    grandchild = client.get("/departments", params={"parent_id": child["id"]}, headers=H1).json()[0]
    r = client.get(f"/departments/{child['id']}/hierarchy", headers=H1).json()
    assert r["department"]["id"] == child["id"]
    assert [p["id"] for p in r["parents"]] == [parent["id"]]
    assert [c["id"] for c in r["children"]] == [grandchild["id"]]


def test_allocation_rules_lifecycle():
    dept = _dept()
    rule = {
        "id": "r-1",
        "department_id": dept["id"],
        "cost_type": "rent",
        "allocation_basis": "headcount",
        "allocation_method": "direct",
    }
    r = client.post("/allocation-rules", json=rule, headers=H1)
    assert r.status_code == 200
    rid = r.json()["id"]

    r = client.get("/allocation-rules", params={"department_id": dept["id"]}, headers=H1)
    assert len(r.json()) == 1

    r = client.delete(f"/allocation-rules/{rid}", headers=H1)
    assert r.status_code == 200
    assert client.get("/allocation-rules", headers=H1).json()[0]["is_active"] is False

    # cross-scope 404
    assert client.delete(f"/allocation-rules/{rid}", headers=H2).status_code == 404


def test_cost_pools_and_allocation_latest_wins():
    d1 = _dept(code="ENG")
    d2 = _dept(code="HR", name="People")
    pool = _pool(excluded=[d2["id"]])
    pid = pool["id"]

    # first run: only ENG targeted (HR excluded)
    r = client.post("/allocate", params={"cost_pool_id": pid, **P}, headers=H1)
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["cost_pool"]["status"] == "closed"
    assert len(body["allocations"]) == 1
    assert body["allocations"][0]["department_id"] == d1["id"]

    # persisted and retrievable
    results = client.get(f"/allocations/{pid}", headers=H1).json()
    assert len(results) == 1

    # rerun: latest-wins (no duplicate accumulation)
    r = client.post("/allocate", params={"cost_pool_id": pid, **P}, headers=H1)
    results = client.get(f"/allocations/{pid}", headers=H1).json()
    assert len(results) == 1

    # pool status persisted
    pool_now = client.get(f"/cost-pools/{pid}", headers=H1).json()
    assert pool_now["status"] == "closed"
    assert pool_now["period_start"].startswith("2026-10-01")

    # unknown pool 404
    assert client.post("/allocate", params={"cost_pool_id": "nope", **P}, headers=H1).status_code == 404


def test_inter_department_bills():
    d1 = _dept(code="ENG")
    d2 = _dept(code="IT", name="IT")
    bill = {
        "id": "b-1",
        "bill_number": "BILL-001",
        "from_department_id": d1["id"],
        "from_department_name": d1["department_name"],
        "to_department_id": d2["id"],
        "to_department_name": d2["department_name"],
        "service_description": "Cloud hosting",
        "service_category": "infrastructure",
        "amount": "1500.50",
        "billing_date": "2026-10-02T00:00:00Z",
        "period_start": "2026-10-01T00:00:00Z",
        "period_end": "2026-10-31T00:00:00Z",
    }
    r = client.post("/inter-department-bills", json=bill, headers=H1)
    assert r.status_code == 200, r.text
    bid = r.json()["id"]

    r = client.get("/inter-department-bills", params={"from_dept_id": d1["id"]}, headers=H1)
    assert len(r.json()) == 1

    r = client.post(f"/inter-department-bills/{bid}/approve", params={"approved_by": "alice"}, headers=H1)
    assert r.status_code == 200
    assert r.json()["status"] == "approved"
    assert r.json()["approved_by"] == "alice"

    # persisted approval
    assert client.get("/inter-department-bills", params={"status": "approved"}, headers=H1).json()[0]["id"] == bid

    # cross-scope 404
    assert (
        client.post(f"/inter-department-bills/{bid}/approve", params={"approved_by": "eve"}, headers=H2).status_code
        == 404
    )


def test_financials_and_reports():
    dept = _dept(code="ENG")
    did = dept["id"]
    pool = _pool(amount="2500.00")
    r = client.post("/allocate", params={"cost_pool_id": pool["id"], **P}, headers=H1)
    assert r.status_code == 200

    # financials reflect allocated costs (accounting service unavailable -> zero direct)
    r = client.get(f"/departments/{did}/financials", params=P, headers=H1)
    assert r.status_code == 200, r.text
    fin = r.json()
    assert fin["department_id"] == did
    assert float(fin["allocated_costs"]) == 2500.0
    assert float(fin["total_expenses"]) == 2500.0
    assert float(fin["net_income"]) == -2500.0

    # performance report derives from the same data
    r = client.get(f"/departments/{did}/performance-report", params=P, headers=H1)
    assert r.status_code == 200
    assert r.json()["financial_summary"]["department_id"] == did
    assert isinstance(r.json()["recommendations"], list)

    # comparison across departments
    hr = _dept(code="HR", name="People")
    r = client.request("GET", "/reports/department-comparison", params=P, json=[did, hr["id"]], headers=H1)
    assert r.status_code == 200
    assert len(r.json()["departments"]) == 2
    assert "total_net_income" in r.json()["summary"]

    # cost distribution over caller's active departments
    r = client.get("/reports/cost-distribution", params=P, headers=H1)
    assert r.status_code == 200
    assert len(r.json()["distribution"]) == 2


def test_cross_user_and_book_isolation():
    d1 = _dept(code="ENG")
    d2 = _dept(code="HR", name="People", headers=H2)
    db_ = _dept(code="OPS", name="Operations", headers=HB)

    # cross-user 404 and listing isolation
    assert client.get(f"/departments/{d1['id']}", headers=H2).status_code == 404
    assert client.get(f"/departments/{d2['id']}", headers=H1).status_code == 404
    assert len(client.get("/departments", headers=H1).json()) == 1

    # Book filter: same caller under a different Book sees only their Book's data
    assert client.get(f"/departments/{d1['id']}", headers=HB).status_code == 404
    assert client.get(f"/departments/{db_['id']}", headers=H1).status_code == 404
    assert len(client.get("/departments", headers=HB).json()) == 1


def test_health_is_caller_scoped():
    _dept(code="ENG")
    body = client.get("/", headers=H1).json()
    assert body["status"] == "healthy"
    assert body["total_departments"] == 1
    assert client.get("/", headers=H2).json()["total_departments"] == 0
