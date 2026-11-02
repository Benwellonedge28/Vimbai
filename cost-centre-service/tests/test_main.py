"""Book-scoping, persistence and behaviour tests for cost-centre-service.

Covers centre creation and updates, cost capture, the period summary (direct,
indirect and the catch-all overhead bucket), listing and filtering, and
cross-user / cross-Book isolation now that all three stores live in Neo4j.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from cost_centre_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("cc_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "cc-user-1", "cc-user-2"
BOOK_A, BOOK_B = "cc-book-a", "cc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _centre(headers=H1, **overrides):
    params = {
        "centre_code": "CC-100",
        "centre_name": "Machining",
        "centre_type": "production",
        "department_id": "dept-1",
        "floor_area": 250.0,
        "number_of_personnel": 8,
        "machine_value": 40000.0,
    }
    params.update(overrides)
    return client.post("/cost-centres/create", params=params, headers=headers).json()


def _cost(centre_id, cost_type="direct", amount=100.0, period="2026-11", headers=H1):
    return client.post(
        "/costs/add",
        params={
            "cost_centre_id": centre_id,
            "cost_code": "C-1",
            "cost_description": "materials",
            "amount": amount,
            "cost_type": cost_type,
            "period": period,
        },
        headers=headers,
    ).json()


def test_create_keeps_the_declared_details():
    centre = _centre()
    assert centre["centre_code"] == "CC-100"
    assert centre["floor_area"] == 250.0
    assert centre["number_of_personnel"] == 8
    assert centre["number_of_requisitions"] == 0
    assert centre["is_service_centre"] is False


def test_update_persists_and_merges_only_the_given_fields():
    centre = _centre()

    updated = client.post(
        f"/cost-centres/{centre['id']}/update",
        params={"floor_area": 400.0, "number_of_requisitions": 12},
        headers=H1,
    ).json()

    assert updated["id"] == centre["id"]
    assert updated["floor_area"] == 400.0
    assert updated["number_of_requisitions"] == 12
    assert updated["number_of_personnel"] == 8

    fetched = client.get(f"/cost-centres/{centre['id']}", headers=H1).json()
    assert fetched["cost_centre"]["floor_area"] == 400.0


def test_update_of_an_unknown_centre_reports_the_original_error():
    body = client.post("/cost-centres/nope/update", params={"floor_area": 1.0}, headers=H1).json()
    assert body == {"error": "Cost centre not found"}


def test_costs_are_listed_with_the_centre_and_totalled():
    centre = _centre()
    _cost(centre["id"], amount=100.0)
    _cost(centre["id"], cost_type="indirect", amount=50.0)
    _cost(centre["id"], cost_type="overhead", amount=25.0)

    body = client.get(f"/cost-centres/{centre['id']}", headers=H1).json()
    assert len(body["costs"]) == 3
    assert body["total_costs"] == 175.0


def test_summary_splits_direct_indirect_and_overhead():
    centre = _centre()
    _cost(centre["id"], amount=100.0)
    _cost(centre["id"], cost_type="indirect", amount=50.0)
    _cost(centre["id"], cost_type="overhead", amount=25.0)
    _cost(centre["id"], cost_type="direct", amount=999.0, period="2026-10")  # other period
    _cost("someone-else", amount=999.0)  # other centre

    summary = client.post(
        "/summary/generate",
        params={"cost_centre_id": centre["id"], "period": "2026-11", "allocated_service_costs": 40},
        headers=H1,
    ).json()

    assert summary["total_direct_costs"] == 100.0
    assert summary["total_indirect_costs"] == 50.0
    assert summary["total_overhead"] == 25.0
    assert summary["allocated_service_costs"] == 40
    assert summary["total_cost"] == 215.0


def test_unknown_cost_type_lands_in_the_overhead_bucket():
    centre = _centre()
    _cost(centre["id"], cost_type="recharge", amount=70.0)

    summary = client.post(
        "/summary/generate",
        params={"cost_centre_id": centre["id"], "period": "2026-11"},
        headers=H1,
    ).json()

    assert summary["total_overhead"] == 70.0
    assert summary["total_cost"] == 70.0


def test_summaries_are_listed_and_filtered():
    centre = _centre()
    for period in ("2026-11", "2026-12"):
        client.post(
            "/summary/generate",
            params={"cost_centre_id": centre["id"], "period": period},
            headers=H1,
        )

    by_period = client.get("/summaries", params={"period": "2026-12"}, headers=H1).json()
    assert [s["period"] for s in by_period["summaries"]] == ["2026-12"]


def test_centre_listing_filters_by_type_department_and_service_flag():
    _centre(centre_code="CC-1", centre_name="Machining")
    _centre(
        centre_code="CC-2",
        centre_name="Maintenance",
        centre_type="service",
        department_id="dept-2",
        is_service_centre=True,
    )

    production = client.get("/cost-centres", params={"centre_type": "production"}, headers=H1).json()
    assert [c["centre_code"] for c in production["cost_centres"]] == ["CC-1"]

    service = client.get("/cost-centres", params={"is_service": True}, headers=H1).json()
    assert [c["centre_code"] for c in service["cost_centres"]] == ["CC-2"]

    dept = client.get("/cost-centres", params={"department_id": "dept-2"}, headers=H1).json()
    assert [c["centre_code"] for c in dept["cost_centres"]] == ["CC-2"]


def test_centres_are_caller_and_book_scoped():
    centre = _centre()

    assert client.get("/cost-centres", headers=H2).json()["cost_centres"] == []
    assert client.get(f"/cost-centres/{centre['id']}", headers=H2).json() == {"error": "Cost centre not found"}
    assert client.get("/cost-centres", headers=HB).json()["cost_centres"] == []


def test_another_tenant_cannot_update_my_centre():
    centre = _centre()

    body = client.post(f"/cost-centres/{centre['id']}/update", params={"floor_area": 1.0}, headers=H2).json()
    assert body == {"error": "Cost centre not found"}
    assert client.get(f"/cost-centres/{centre['id']}", headers=H1).json()["cost_centre"]["floor_area"] == 250.0


def test_summary_only_counts_my_own_costs_for_a_shared_centre_id():
    centre = _centre()
    _cost(centre["id"], amount=100.0)
    _cost(centre["id"], amount=500.0, headers=H2)

    mine = client.post(
        "/summary/generate", params={"cost_centre_id": centre["id"], "period": "2026-11"}, headers=H1
    ).json()
    theirs = client.post(
        "/summary/generate", params={"cost_centre_id": centre["id"], "period": "2026-11"}, headers=H2
    ).json()

    assert mine["total_direct_costs"] == 100.0
    assert theirs["total_direct_costs"] == 500.0


def test_caller_header_is_required():
    assert client.get("/cost-centres").status_code == 422


def test_health_reports_the_engine():
    body = client.get("/health").json()
    assert body["service"] == "cost-centre-service"
    assert body["calculation_engine"] in ("rust", "python")
