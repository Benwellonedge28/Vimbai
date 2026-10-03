"""Book-scoping and persistence tests for treasury-risk-service (fake Neo4j harness)."""

import importlib.util
import os

import main  # noqa: F401 (bootstraps the treasury_risk_service package alias)
import pytest
from fastapi.testclient import TestClient
from treasury_risk_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("trisk_root_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "trisk-user-1", "trisk-user-2"
BOOK_A, BOOK_B = "trisk-book-a", "trisk-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def test_health():
    assert client.get("/health").json()["service"] == "treasury-risk-service"


def test_exposure_scoping():
    r = client.post(
        "/exposures",
        params={"exposure_type": "fx", "currency": "USD", "notional_amount": 100000.0, "description": "USD/ZWL"},
        headers=H1,
    )
    assert r.status_code == 200
    client.post(
        "/exposures",
        params={"exposure_type": "credit", "currency": "USD", "notional_amount": 50000.0},
        headers=H1,
    )
    # invalid type
    assert (
        client.post(
            "/exposures",
            params={"exposure_type": "weather", "currency": "USD", "notional_amount": 1.0},
            headers=H1,
        ).status_code
        == 400
    )

    assert len(client.get("/exposures", headers=H1).json()) == 2
    assert len(client.get("/exposures", params={"exposure_type": "fx"}, headers=H1).json()) == 1
    # foreign caller + Book-gated
    assert client.get("/exposures", headers=H2).json() == []
    other_book = dict(H1)
    other_book["X-Book-ID"] = BOOK_B
    assert client.get("/exposures", headers=other_book).json() == []


def test_var_persists_and_scopes():
    # invalid confidence
    assert (
        client.post(
            "/var",
            params={"portfolio_value": 1000000.0, "confidence_level": 0.50},
            headers=H1,
        ).status_code
        == 400
    )

    r = client.post(
        "/var",
        params={
            "portfolio_value": 1000000.0,
            "confidence_level": 0.95,
            "holding_period_days": 1,
            "daily_volatility": 0.01,
        },
        headers=H1,
    ).json()
    assert r["var_amount"] == 16450.0
    assert r["var_pct"] == 1.645

    # persisted + scoped
    var_list = client.get("/var", headers=H1).json()
    assert len(var_list) == 1
    assert var_list[0]["var_amount"] == 16450.0
    assert client.get("/var", headers=H2).json() == []


def test_scenarios_and_stress_scoping():
    s = client.post(
        "/scenarios",
        params={"name": "Market Crash", "description": "Severe", "shock_type": "market_crash", "shock_magnitude": 30.0},
        headers=H1,
    ).json()
    # invalid shock type
    assert (
        client.post(
            "/scenarios",
            params={"name": "Bad", "description": "x", "shock_type": "earthquake", "shock_magnitude": 1.0},
            headers=H1,
        ).status_code
        == 400
    )

    # foreign caller cannot run U1's scenario
    assert client.post(f"/scenarios/{s['id']}/run", params={"portfolio_value": 100000.0}, headers=H2).status_code == 404

    res = client.post(f"/scenarios/{s['id']}/run", params={"portfolio_value": 100000.0}, headers=H1).json()
    assert res["impact"] == -30000.0
    assert res["portfolio_value_after"] == 70000.0
    assert res["impact_pct"] == -30.0

    # results scoped + scenario filter + tail limit
    assert len(client.get("/stress-results", headers=H1).json()) == 1
    assert len(client.get("/stress-results", params={"scenario_id": s["id"]}, headers=H1).json()) == 1
    assert client.get("/stress-results", params={"limit": 1}, headers=H1).json()[0]["impact"] == -30000.0
    assert client.get("/stress-results", headers=H2).json() == []
    other_book = dict(H1)
    other_book["X-Book-ID"] = BOOK_B
    assert client.get("/stress-results", headers=other_book).json() == []


def test_dashboard_scoped():
    client.post(
        "/exposures",
        params={"exposure_type": "fx", "currency": "USD", "notional_amount": 100000.0},
        headers=H1,
    )
    client.post("/var", params={"portfolio_value": 1000000.0}, headers=H1)
    s = client.post(
        "/scenarios",
        params={"name": "IR Up", "description": "d", "shock_type": "interest_rate_up", "shock_magnitude": 200.0},
        headers=H1,
    ).json()
    client.post(f"/scenarios/{s['id']}/run", params={"portfolio_value": 500000.0}, headers=H1)

    d = client.get("/dashboard", headers=H1).json()
    assert d["total_exposures"] == 1
    assert d["total_notional"] == 100000.0
    assert d["by_type"] == {"fx": 100000.0}
    assert d["var_calculations"] == 1
    assert d["latest_var"] == 16450.0
    assert d["stress_scenarios"] == 1
    assert d["stress_tests_run"] == 1

    # foreign caller sees an empty dashboard
    d2 = client.get("/dashboard", headers=H2).json()
    assert d2["total_exposures"] == 0
    assert d2["latest_var"] == 0
