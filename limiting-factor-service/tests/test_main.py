"""Book-scoping, persistence and optimisation tests for limiting-factor-service.

Covers the scarce-resource production plan (ranking by contribution per factor
unit, then filling demand until the resource runs out), the ranking-only
comparison, listing and lookup, and cross-user / cross-Book isolation now that
analyses live in Neo4j.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from fastapi.testclient import TestClient
from limiting_factor_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("lf_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "lf-user-1", "lf-user-2"
BOOK_A, BOOK_B = "lf-book-a", "lf-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

# Beta earns 8 per machine hour, Alpha 5, so Beta is filled first.
PRODUCTS = [
    {"product_id": "A", "product_name": "Alpha", "demand": 600, "contribution_per_unit": 10, "factor_per_unit": 2},
    {"product_id": "B", "product_name": "Beta", "demand": 300, "contribution_per_unit": 8, "factor_per_unit": 1},
]


def _analyze(headers=H1, total_available=1000.0, products=None):
    return client.post(
        "/analyze",
        params={
            "limiting_factor": "machine hours",
            "factor_type": "machine_hours",
            "total_available": total_available,
        },
        json=products if products is not None else PRODUCTS,
        headers=headers,
    ).json()


def test_plan_ranks_by_contribution_per_factor_unit():
    body = _analyze()

    assert [p["product_id"] for p in body["products"]] == ["B", "A"]
    assert [p["ranking"] for p in body["products"]] == [1, 2]
    assert body["products"][0]["contribution_per_factor_unit"] == 8.0
    assert body["products"][1]["contribution_per_factor_unit"] == 5.0


def test_plan_fills_demand_until_the_resource_runs_out():
    body = _analyze()

    assert body["optimal_production_plan"] == {"A": 350.0, "B": 300.0}
    assert body["factor_used"] == 1000.0
    assert body["factor_remaining"] == 0.0
    assert body["total_contribution"] == 5900.0


def test_plan_leaves_the_resource_unused_when_demand_is_small():
    body = _analyze(total_available=5000.0)

    assert body["optimal_production_plan"] == {"A": 600.0, "B": 300.0}
    assert body["factor_used"] == 1500.0
    assert body["factor_remaining"] == 3500.0
    assert body["total_contribution"] == 8400.0


def test_analysis_persists_and_is_retrievable():
    created = _analyze()

    listed = client.get("/analyses", headers=H1).json()["analyses"]
    assert [a["id"] for a in listed] == [created["id"]]
    assert listed[0]["optimal_production_plan"] == {"A": 350.0, "B": 300.0}
    assert [p["product_id"] for p in listed[0]["products"]] == ["B", "A"]

    fetched = client.get(f"/analyses/{created['id']}", headers=H1).json()
    assert fetched["total_contribution"] == 5900.0


def test_listing_filters_by_limiting_factor():
    _analyze()
    _analyze(
        products=[
            {"product_id": "C", "product_name": "Gamma", "demand": 1, "contribution_per_unit": 1, "factor_per_unit": 1}
        ]
    )

    matching = client.get("/analyses", params={"limiting_factor": "machine hours"}, headers=H1).json()["analyses"]
    assert len(matching) == 2
    assert client.get("/analyses", params={"limiting_factor": "labour"}, headers=H1).json()["analyses"] == []


def test_unknown_analysis_reports_the_original_error():
    assert client.get("/analyses/nope", headers=H1).json() == {"error": "Analysis not found"}


def test_compare_products_ranks_without_persisting():
    body = client.post(
        "/compare-products",
        json=[
            {"product_id": "X", "product_name": "Xray", "contribution_per_unit": 4, "factor_per_unit": 2},
            {"product_id": "Y", "product_name": "Yankee", "contribution_per_unit": 9, "factor_per_unit": 3},
        ],
        headers=H1,
    ).json()

    assert [p["product_id"] for p in body["ranked_products"]] == ["Y", "X"]
    assert client.get("/analyses", headers=H1).json()["analyses"] == []


def test_analyses_are_caller_and_book_scoped():
    created = _analyze()

    assert client.get("/analyses", headers=H2).json()["analyses"] == []
    assert client.get("/analyses", headers=HB).json()["analyses"] == []
    assert client.get(f"/analyses/{created['id']}", headers=H2).json() == {"error": "Analysis not found"}


def test_caller_header_is_required_for_the_listing():
    assert client.get("/analyses").status_code == 422


def test_health_reports_the_engine():
    body = client.get("/health").json()
    assert body["service"] == "limiting-factor-service"
    assert body["calculation_engine"] in ("rust", "python")
