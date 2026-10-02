"""Book-scoping and persistence tests for scenario-modeling-service (fake Neo4j harness).

Covers: scenario/rule CRUD persistence, ownership and Book isolation,
what-if + sensitivity ownership gating, compare isolation, and
cross-scope 404s (the rules store previously had NO user dimension
and analysis ran against ANY scenario id).
"""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from scenario_modeling_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("sm_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "sm-user-1", "sm-user-2"
BOOK_A, BOOK_B = "sm-book-a", "sm-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_OTHER_BOOK = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _scenario(name="FY2027 budget", scenario_type="budget_forecast", variables=None, **kw):
    body = {
        "name": name,
        "description": "test scenario",
        "scenario_type": scenario_type,
        "base_date": "2026-01-01",
        "end_date": "2026-12-31",
        "variables": (
            variables
            if variables is not None
            else [
                {"name": "revenue", "current_value": 100000},
                {"name": "costs", "current_value": 40000},
            ]
        ),
        "rules": [],
        "assumptions": {"base_revenue": 100000},
    }
    body.update(kw)
    return body


def _rule(name="Boost revenue", **kw):
    body = {
        "name": name,
        "description": "test rule",
        "conditions": [{"field": "revenue", "operator": "greater_than", "value": 50000}],
        "actions": [{"action_type": "set_value", "target_field": "flag", "value": "high"}],
        "priority": 10,
        "enabled": True,
    }
    body.update(kw)
    return body


# ---------------------------------------------------------------------------
# Health
# ---------------------------------------------------------------------------


def test_health():
    r = client.get("/health")
    assert r.status_code == 200
    assert r.json()["status"] == "healthy"


def test_identity_header_required():
    r = client.get("/scenarios/")
    assert r.status_code == 422


# ---------------------------------------------------------------------------
# Scenario CRUD + isolation
# ---------------------------------------------------------------------------


def test_scenario_crud_persists():
    r = client.post("/scenarios/", json=_scenario(), headers=H1)
    assert r.status_code == 201, r.text
    sid = r.json()["id"]
    assert r.json()["status"] == "draft"

    # persists across a fresh client (same graph) - survives restart
    r2 = TestClient(app).get(f"/scenarios/{sid}", headers=H1)
    assert r2.status_code == 200
    assert r2.json()["name"] == "FY2027 budget"


def test_scenario_user_isolation():
    r = client.post("/scenarios/", json=_scenario(), headers=H1)
    sid = r.json()["id"]

    assert client.get("/scenarios/", headers=H2).json() == []
    assert client.get(f"/scenarios/{sid}", headers=H2).status_code == 404
    assert client.put(f"/scenarios/{sid}", json={"name": "hijacked"}, headers=H2).status_code == 404
    assert client.delete(f"/scenarios/{sid}", headers=H2).status_code == 404
    # original still intact after the attempts
    assert client.get(f"/scenarios/{sid}", headers=H1).json()["name"] == "FY2027 budget"


def test_scenario_book_isolation():
    sid = client.post("/scenarios/", json=_scenario(), headers=H1).json()["id"]

    # same user, other Book: invisible
    assert client.get(f"/scenarios/{sid}", headers=H1_OTHER_BOOK).status_code == 404
    assert client.get("/scenarios/", headers=H1_OTHER_BOOK).json() == []
    # personal (no Book) view still sees own data
    assert client.get(f"/scenarios/{sid}", headers=H1_PERSONAL).status_code == 200
    # Book-A view sees it
    assert client.get(f"/scenarios/{sid}", headers=H1).status_code == 200


def test_scenario_update_and_delete_persist():
    sid = client.post("/scenarios/", json=_scenario(), headers=H1).json()["id"]

    r = client.put(f"/scenarios/{sid}", json={"name": "Renamed", "status": "active"}, headers=H1)
    assert r.status_code == 200
    fresh = TestClient(app)
    got = fresh.get(f"/scenarios/{sid}", headers=H1).json()
    assert got["name"] == "Renamed"
    assert got["status"] == "active"

    assert client.delete(f"/scenarios/{sid}", headers=H1).status_code == 204
    assert client.get(f"/scenarios/{sid}", headers=H1).status_code == 404


def test_scenario_type_filter():
    client.post("/scenarios/", json=_scenario(name="b1", scenario_type="budget_forecast"), headers=H1)
    client.post("/scenarios/", json=_scenario(name="r1", scenario_type="revenue_projection"), headers=H1)
    listed = client.get("/scenarios/", params={"scenario_type": "budget_forecast"}, headers=H1).json()
    assert [s["name"] for s in listed] == ["b1"]


# ---------------------------------------------------------------------------
# Rules (previously a fully global store - no user dimension at all)
# ---------------------------------------------------------------------------


def test_rules_are_caller_owned():
    rule = client.post("/rules/", json=_rule(), headers=H1).json()
    rid = rule["id"]

    assert client.get("/rules/", headers=H2).json() == []
    assert client.get(f"/rules/{rid}", headers=H2).status_code == 404
    assert client.put(f"/rules/{rid}", json={"name": "hijacked"}, headers=H2).status_code == 404
    assert client.delete(f"/rules/{rid}", headers=H2).status_code == 404
    assert client.get(f"/rules/{rid}", headers=H1).json()["name"] == "Boost revenue"


def test_rules_book_isolation_and_update():
    rid = client.post("/rules/", json=_rule(), headers=H1).json()["id"]
    assert client.get(f"/rules/{rid}", headers=H1_OTHER_BOOK).status_code == 404
    assert client.get("/rules/", headers=H1_OTHER_BOOK).json() == []

    r = client.put(f"/rules/{rid}", json={"enabled": False, "priority": 1}, headers=H1)
    assert r.status_code == 200
    got = TestClient(app).get("/rules/", params={"enabled_only": True}, headers=H1).json()
    assert got == []
    got_all = TestClient(app).get("/rules/", headers=H1).json()
    assert got_all[0]["enabled"] is False
    assert got_all[0]["priority"] == 1


def test_scenario_create_validates_rule_ownership():
    foreign = client.post("/rules/", json=_rule(name="foreign"), headers=H2).json()["id"]
    r = client.post("/scenarios/", json=_scenario(rules=[foreign]), headers=H1)
    assert r.status_code == 404
    assert foreign in r.json()["detail"]

    own = client.post("/rules/", json=_rule(name="own"), headers=H1).json()["id"]
    r = client.post("/scenarios/", json=_scenario(rules=[own]), headers=H1)
    assert r.status_code == 201
    assert r.json()["rules"] == [own]


# ---------------------------------------------------------------------------
# What-If analysis (previously ran against ANY scenario id)
# ---------------------------------------------------------------------------


def test_whatif_requires_scenario_ownership():
    sid = client.post("/scenarios/", json=_scenario(), headers=H1).json()["id"]
    body = {"scenario_id": sid, "variable_changes": {"revenue": 120000}}
    r = client.post("/what-if/", json=body, headers=H2)
    assert r.status_code == 404
    r = client.post("/what-if/", json=body, headers=H1_OTHER_BOOK)
    assert r.status_code == 404


def test_whatif_computes_and_persists():
    sid = client.post("/scenarios/", json=_scenario(), headers=H1).json()["id"]
    r = client.post("/what-if/", json={"scenario_id": sid, "variable_changes": {"revenue": 120000}}, headers=H1)
    assert r.status_code == 201, r.text
    data = r.json()
    assert float(data["original_outcome"]) == 140000.0  # 100k + 40k
    assert float(data["new_outcome"]) == 160000.0
    assert float(data["variance"]) == 20000.0

    aid = data["analysis_id"]
    fresh = TestClient(app)
    got = fresh.get(f"/what-if/{aid}", headers=H1)
    assert got.status_code == 200
    assert float(got.json()["new_outcome"]) == 160000.0
    # another user cannot read the result
    assert fresh.get(f"/what-if/{aid}", headers=H2).status_code == 404
    # per-scenario listing is caller-owned
    assert fresh.get(f"/what-if/scenario/{sid}", headers=H2).json() == []
    assert len(fresh.get(f"/what-if/scenario/{sid}", headers=H1).json()) == 1


# ---------------------------------------------------------------------------
# Sensitivity analysis
# ---------------------------------------------------------------------------


def test_sensitivity_requires_ownership_and_persists():
    sid = client.post("/scenarios/", json=_scenario(), headers=H1).json()["id"]
    body = {"scenario_id": sid, "variable_name": "revenue", "min_change": -10000, "max_change": 10000, "steps": 5}
    assert client.post("/sensitivity/", json=body, headers=H2).status_code == 404
    r = client.post("/sensitivity/", json=body, headers=H1)
    assert r.status_code == 201, r.text
    assert len(r.json()["outcomes"]) == 5

    # unknown variable still 404s (original semantics)
    bad = dict(body, variable_name="nope")
    assert client.post("/sensitivity/", json=bad, headers=H1).status_code == 404


# ---------------------------------------------------------------------------
# Compare
# ---------------------------------------------------------------------------


def test_compare_requires_ownership():
    s1 = client.post("/scenarios/", json=_scenario(name="low"), headers=H1).json()["id"]
    s2 = client.post("/scenarios/", json=_scenario(name="high"), headers=H1).json()["id"]

    r = client.post("/compare/", json=[s1, s2], headers=H2)
    assert r.status_code == 404

    r = client.post("/compare/", json=[s1, s2], headers=H1)
    assert r.status_code == 200
    assert r.json()["best_case"] is not None
    assert r.json()["worst_case"] is not None

    # a scenario id from another user inside an otherwise-own list 404s
    s3 = client.post("/scenarios/", json=_scenario(name="u2s"), headers=H2).json()["id"]
    assert client.post("/compare/", json=[s1, s3], headers=H1).status_code == 404
