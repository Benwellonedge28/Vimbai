"""Book-scoping and persistence tests for scenario-analysis-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from scenario_analysis_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("sa_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "sa-user-1", "sa-user-2"
BOOK_A, BOOK_B = "sa-book-a", "sa-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _payload(company="co-sa", name="Base", rev=150000, exp=100000, **kw):
    payload = {"company_id": company, "name": name, "projected_revenue": rev, "projected_expenses": exp}
    payload.update(kw)
    return payload


def test_create_list_persist():
    resp = client.post("/scenarios", json=_payload(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["net_projection"] == 50000.0
    assert data["book_id"] == BOOK_A

    listed = client.get("/scenarios/co-sa", headers=H1).json()
    assert listed["total"] == 1
    # persists across requests
    assert client.get("/scenarios/co-sa", headers=H1).json()["total"] == 1

    # other user sees nothing
    assert client.get("/scenarios/co-sa", headers=H2).json()["total"] == 0


def test_compare_scopes():
    for name, rev in [("Best", 200000), ("Worst", 100000)]:
        client.post("/scenarios", json=_payload(company="co-cmp", name=name, rev=rev, exp=80000), headers=H1)
    cmp_data = client.get("/compare/co-cmp", headers=H1).json()
    assert cmp_data["best_case"] == "Best"
    assert cmp_data["worst_case"] == "Worst"
    assert cmp_data["range"] == 100000.0

    # fewer than 2 in other user's view
    assert client.get("/compare/co-cmp", headers=H2).json()["comparison"] == "Need at least 2 scenarios"
    # cross-user scenario never counted
    client.post("/scenarios", json=_payload(company="co-cmp", name="U2 Case", rev=900000, exp=1000), headers=H2)
    cmp_data = client.get("/compare/co-cmp", headers=H1).json()
    assert cmp_data["best_case"] == "Best"  # U2's fat scenario invisible


def test_book_a_b_isolation():
    client.post("/scenarios", json=_payload(company="co-a", name="A Case"), headers=H1)
    client.post(
        "/scenarios", json=_payload(company="co-a", name="B Case"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}
    )
    names_a = [s["name"] for s in client.get("/scenarios/co-a", headers=H1).json()["scenarios"]]
    assert names_a == ["A Case"]
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    names_b = [s["name"] for s in client.get("/scenarios/co-a", headers=other_book).json()["scenarios"]]
    assert names_b == ["B Case"]
    # personal sees both
    names_p = [s["name"] for s in client.get("/scenarios/co-a", headers=H1_PERSONAL).json()["scenarios"]]
    assert set(names_p) == {"A Case", "B Case"}


def test_analyze_stateless():
    payload = {
        "company_id": "co-x",
        "base_revenue": 1000000,
        "base_cost": 700000,
        "base_interest": 20000,
        "base_depreciation": 50000,
        "best_case": {"revenue_growth": 0.2, "cost_growth": 0.03, "description": "Optimistic"},
        "base_case": {"revenue_growth": 0.1, "cost_growth": 0.05, "description": "Expected"},
        "worst_case": {"revenue_growth": -0.1, "cost_growth": 0.08, "description": "Pessimistic"},
    }
    resp = client.post("/analyze", json=payload, headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["best_case"]["net_income"] > data["worst_case"]["net_income"]
    assert len(data["recommendation"]) > 0
    # stored scenarios unaffected
    assert client.get("/scenarios/co-x", headers=H1).json()["total"] == 0


def test_x_user_id_required():
    assert client.post("/scenarios", json=_payload()).status_code in (401, 403, 422)
    assert client.get("/scenarios/co-sa").status_code in (401, 403, 422)
    assert client.post(
        "/analyze",
        json={"company_id": "c", "base_revenue": 1, "base_cost": 1, "best_case": {}, "base_case": {}, "worst_case": {}},
    ).status_code in (401, 403, 422)
