"""Book-scoping, persistence and behaviour tests for overhead-apportionment-service.

Covers apportionment by basis, the empty-basis case, batch runs, listing and
filtering, the cost-centre summary, and cross-user / cross-Book isolation now
that overheads and results live in Neo4j.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from fastapi.testclient import TestClient
from overhead_apportionment_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("oa_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "oa-user-1", "oa-user-2"
BOOK_A, BOOK_B = "oa-book-a", "oa-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

CENTRES = [
    {"cost_centre_id": "c1", "cost_centre_name": "Machining", "floor_area": 300, "machine_hours": 90},
    {"cost_centre_id": "c2", "cost_centre_name": "Assembly", "floor_area": 700, "machine_hours": 10},
]


def _overhead(basis="floor_area", amount=10000.0, headers=H1, name="Factory rent"):
    return client.post(
        "/overheads/add",
        params={
            "overhead_name": name,
            "overhead_code": "OH-1",
            "total_amount": amount,
            "basis": basis,
        },
        headers=headers,
    ).json()


def test_apportion_splits_by_the_named_basis():
    overhead = _overhead()

    body = client.post(
        "/apportion",
        params={"overhead_id": overhead["id"], "period": "2026-11"},
        json=CENTRES,
        headers=H1,
    ).json()

    splits = body["cost_centre_apportionments"]
    assert body["total_overhead"] == 10000.0
    assert [s["cost_centre_id"] for s in splits] == ["c1", "c2"]
    assert splits[0]["proportion"] == 0.3
    assert splits[0]["apportioned_amount"] == 3000.0
    assert splits[1]["apportioned_amount"] == 7000.0


def test_apportion_uses_whichever_basis_the_overhead_declares():
    overhead = _overhead(basis="machine_hours")

    splits = client.post(
        "/apportion",
        params={"overhead_id": overhead["id"], "period": "2026-11"},
        json=CENTRES,
        headers=H1,
    ).json()["cost_centre_apportionments"]

    assert splits[0]["apportioned_amount"] == 9000.0
    assert splits[0]["basis"] == "machine_hours"


def test_zero_basis_total_yields_no_split():
    overhead = _overhead()

    body = client.post(
        "/apportion",
        params={"overhead_id": overhead["id"], "period": "2026-11"},
        json=[{"cost_centre_id": "c1", "cost_centre_name": "Machining"}],
        headers=H1,
    ).json()

    assert body["total_overhead"] == 10000.0
    assert body["cost_centre_apportionments"] == []


def test_unknown_overhead_reports_the_original_error_shape():
    body = client.post(
        "/apportion",
        params={"overhead_id": "nope", "period": "2026-11"},
        json=CENTRES,
        headers=H1,
    ).json()
    assert body == {"error": "Overhead not found"}


def test_batch_apportions_every_overhead_for_the_caller():
    first = _overhead(name="Factory rent")
    second = _overhead(basis="machine_hours", amount=5000.0, name="Machine power")

    body = client.post(
        "/batch-apportion",
        params={"period": "2026-11"},
        json=CENTRES,
        headers=H1,
    ).json()

    assert body["period"] == "2026-11"
    assert {r["overhead_id"] for r in body["apportionment_results"]} == {first["id"], second["id"]}


def test_listings_filter_by_overhead_and_period():
    overhead = _overhead()
    client.post(
        "/apportion",
        params={"overhead_id": overhead["id"], "period": "2026-11"},
        json=CENTRES,
        headers=H1,
    )
    client.post(
        "/apportion",
        params={"overhead_id": overhead["id"], "period": "2026-12"},
        json=CENTRES,
        headers=H1,
    )

    by_period = client.get("/apportionments", params={"period": "2026-11"}, headers=H1).json()
    assert [a["period"] for a in by_period["apportionments"]] == ["2026-11"]

    by_overhead = client.get("/apportionments", params={"overhead_id": overhead["id"]}, headers=H1).json()
    assert len(by_overhead["apportionments"]) == 2


def test_cost_centre_summary_totals_the_period():
    """The original read the unused list and always answered empty; that bug is fixed."""
    rent = _overhead(name="Factory rent")
    power = _overhead(basis="machine_hours", amount=5000.0, name="Machine power")
    for overhead in (rent, power):
        client.post(
            "/apportion",
            params={"overhead_id": overhead["id"], "period": "2026-11"},
            json=CENTRES,
            headers=H1,
        )

    summary = client.get("/cost-centre-summary/2026-11", headers=H1).json()["cost_centre_summary"]
    by_centre = {row["cost_centre_id"]: row for row in summary}

    assert by_centre["c1"]["total_overhead"] == 7500.0
    assert by_centre["c2"]["total_overhead"] == 7500.0
    assert len(by_centre["c1"]["breakdown"]) == 2

    assert client.get("/cost-centre-summary/2030-01", headers=H1).json()["cost_centre_summary"] == []


def test_overheads_are_caller_scoped():
    _overhead()

    assert client.get("/overheads", headers=H1).json()["overheads"] != []
    assert client.get("/overheads", headers=H2).json()["overheads"] == []
    assert client.get("/overheads", headers=HB).json()["overheads"] == []


def test_another_tenant_cannot_apportion_or_read_my_results():
    overhead = _overhead()
    client.post(
        "/apportion",
        params={"overhead_id": overhead["id"], "period": "2026-11"},
        json=CENTRES,
        headers=H1,
    )

    stranger = client.post(
        "/apportion",
        params={"overhead_id": overhead["id"], "period": "2026-11"},
        json=CENTRES,
        headers=H2,
    ).json()
    assert stranger == {"error": "Overhead not found"}
    assert client.get("/apportionments", headers=H2).json()["apportionments"] == []

    other_book = client.post(
        "/batch-apportion",
        params={"period": "2026-11"},
        json=CENTRES,
        headers=HB,
    ).json()
    assert other_book["apportionment_results"] == []


def test_caller_header_is_required():
    assert client.get("/overheads").status_code == 422


def test_health_still_reports_the_service():
    body = client.get("/health").json()
    assert body["service"] == "overhead-apportionment-service"
    assert body["status"] == "healthy"
    assert body["calculation_engine"] in ("rust", "python")
