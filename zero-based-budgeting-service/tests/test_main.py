"""Book-scoping and persistence tests for zero-based-budgeting-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from zero_based_budgeting_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("zbb_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "zbb-user-1", "zbb-user-2"
BOOK_A, BOOK_B = "zbb-book-a", "zbb-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _pkg_payload(company="co-zbb", name="IT Budget", dept="IT", with_items=True):
    payload = {"company_id": company, "period": "2026-Q1", "name": name, "department": dept}
    if with_items:
        payload["items"] = [
            {
                "department": dept,
                "category": "software",
                "description": "Licenses",
                "amount": 50000,
                "justification": "Required for ops",
            }
        ]
    return payload


def test_create_package_totals_and_persist():
    resp = client.post("/packages", json=_pkg_payload(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["total_amount"] == 50000.0
    assert data["status"] == "draft"
    assert data["book_id"] == BOOK_A

    listed = client.get("/packages/co-zbb", headers=H1).json()
    assert listed["total"] == 1
    # persists across requests
    assert client.get("/packages/co-zbb", headers=H1).json()["total"] == 1
    # other user sees nothing
    assert client.get("/packages/co-zbb", headers=H2).json()["total"] == 0

    # department + status filters
    assert client.get("/packages/co-zbb", params={"department": "IT"}, headers=H1).json()["total"] == 1
    assert client.get("/packages/co-zbb", params={"department": "HR"}, headers=H1).json()["total"] == 0
    assert client.get("/packages/co-zbb", params={"status_filter": "draft"}, headers=H1).json()["total"] == 1
    assert client.get("/packages/co-zbb", params={"status_filter": "approved"}, headers=H1).json()["total"] == 0


def test_add_item_and_summary():
    pkg_id = client.post("/packages", json=_pkg_payload(company="co-sum"), headers=H1).json()["id"]
    resp = client.post(
        f"/packages/{pkg_id}/items",
        json={
            "department": "IT",
            "category": "hardware",
            "description": "Laptops",
            "amount": 20000,
            "justification": "New hires",
        },
        headers=H1,
    )
    assert resp.status_code == 200, resp.text
    assert resp.json()["total_amount"] == 70000.0

    summary = client.get("/summary/co-sum", headers=H1).json()
    assert summary["total_packages"] == 1
    assert summary["total_budget"] == 70000.0
    assert summary["by_department"] == {"IT": 70000.0}
    assert summary["by_status"] == {"draft": 1}
    # other user summary empty
    assert client.get("/summary/co-sum", headers=H2).json()["total_packages"] == 0


def test_update_status_scopes():
    pkg_id = client.post("/packages", json=_pkg_payload(company="co-st"), headers=H1).json()["id"]
    # cross-user status update 404
    assert client.put(f"/packages/{pkg_id}/status", params={"status": "approved"}, headers=H2).status_code == 404
    resp = client.put(
        f"/packages/{pkg_id}/status",
        params={"status": "approved", "reviewer": "CFO", "notes": "OK"},
        headers=H1,
    )
    assert resp.status_code == 200
    assert resp.json() == {"id": pkg_id, "status": "approved"}
    # persists
    pkgs = client.get("/packages/co-st", params={"status_filter": "approved"}, headers=H1).json()["packages"]
    assert pkgs[0]["reviewer"] == "CFO"
    assert pkgs[0]["review_notes"] == "OK"
    # cross-Book update 404
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.put(f"/packages/{pkg_id}/status", params={"status": "draft"}, headers=other_book).status_code == 404


def test_item_priority():
    pkg = client.post("/packages", json=_pkg_payload(company="co-pr"), headers=H1).json()
    item_id = pkg["items"][0]["id"]
    # priority validation
    assert client.put(f"/items/{item_id}/priority", params={"priority": 9}, headers=H1).status_code == 400
    resp = client.put(f"/items/{item_id}/priority", params={"priority": 1, "status": "approved"}, headers=H1)
    assert resp.status_code == 200
    assert resp.json() == {"item_id": item_id, "priority": 1, "status": "approved"}
    # persists
    stored = client.get("/packages/co-pr", headers=H1).json()["packages"][0]
    assert stored["items"][0]["priority"] == 1
    assert stored["items"][0]["status"] == "approved"
    # cross-user item 404
    assert client.put(f"/items/{item_id}/priority", params={"priority": 2}, headers=H2).status_code == 404


def test_book_a_b_isolation():
    client.post("/packages", json=_pkg_payload(company="co-a", name="A Pkg"), headers=H1)
    client.post(
        "/packages", json=_pkg_payload(company="co-a", name="B Pkg"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}
    )
    names_a = [p["name"] for p in client.get("/packages/co-a", headers=H1).json()["packages"]]
    assert names_a == ["A Pkg"]
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    names_b = [p["name"] for p in client.get("/packages/co-a", headers=other_book).json()["packages"]]
    assert names_b == ["B Pkg"]
    # personal sees both
    names_p = [p["name"] for p in client.get("/packages/co-a", headers=H1_PERSONAL).json()["packages"]]
    assert set(names_p) == {"A Pkg", "B Pkg"}


def test_x_user_id_required():
    assert client.post("/packages", json=_pkg_payload()).status_code in (401, 403, 422)
    assert client.get("/packages/co-zbb").status_code in (401, 403, 422)
