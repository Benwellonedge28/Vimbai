"""Book-scoping, persistence and variance tests for over-under-absorption-service.

Covers over and under absorption, the cause heuristic, the comprehensive
spending/volume variance split, the stateless helpers (simple calculation,
spending/volume/capacity variance, adjustment treatment), listing filters, and
cross-user / cross-Book isolation now that both stores live in Neo4j.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from fastapi.testclient import TestClient
from over_under_absorption_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("oua_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "oua-user-1", "oua-user-2"
BOOK_A, BOOK_B = "oua-book-a", "oua-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

BASE = {
    "cost_centre_name": "Machining",
    "cost_centre_id": "CC-1",
    "period": "2026-11",
    "budgeted_overhead": 50000.0,
    "actual_overhead": 48000.0,
    "budgeted_base_units": 5000.0,
    "actual_base_units": 4500.0,
    "absorption_rate": 10.0,
}


def _calculate(headers=H1, **overrides):
    params = dict(BASE)
    params.update(overrides)
    return client.post("/calculate", params=params, headers=headers).json()


def test_under_absorption_record():
    body = _calculate()
    # absorbed 45000 against 48000 actual
    assert body["overhead_absorbed"] == 45000.0
    assert body["under_absorption"] == 3000.0
    assert body["over_absorption"] == 0
    assert body["total_variance"] == 3000.0


def test_over_absorption_record():
    body = _calculate(actual_overhead=40000.0)
    assert body["overhead_absorbed"] == 45000.0
    assert body["over_absorption"] == 5000.0
    assert body["under_absorption"] == 0
    assert body["total_variance"] == 5000.0


def test_cause_picks_the_larger_of_spending_and_volume():
    # spending variance 2000, total variance -3000 -> volume direction dominates
    assert _calculate()["cause"] == "volume"
    # heavy spending overspend dominates
    assert _calculate(actual_overhead=60000.0)["cause"] == "overhead_spending"


def test_simple_calculation_is_stateless():
    body = client.post(
        "/simple-calculation", params={"overhead_absorbed": 45000.0, "actual_overhead": 48000.0}, headers=H1
    ).json()
    # the simple form reports a magnitude plus a status, not a signed variance
    assert body["variance"] == 3000.0
    assert body["status"] == "under_absorbed"
    assert client.get("/records", headers=H1).json()["records"] == []


def test_comprehensive_analysis_splits_spending_and_volume():
    body = client.post("/comprehensive-analysis", params=BASE, headers=H1).json()

    assert body["overhead_absorbed"] == 45000.0
    assert body["spending_variance"] == 2000.0
    assert body["spending_favorable"] is True
    # absorbed 45000 vs budgeted absorbed 50000
    assert body["volume_variance"] == -5000.0
    assert body["volume_favorable"] is False
    assert body["total_variance"] == -3000.0
    assert body["status"] == "under_absorbed"
    assert client.get("/analyses", headers=H1).json()["analyses"][0]["over_or_under"] == "under"


def test_adjustment_treatment_is_stateless():
    body = client.post(
        "/adjust-over-under",
        params={"over_absorption": 5000.0, "under_absorption": 2000.0, "write_off_to_costing": True},
        headers=H1,
    ).json()
    assert body["net_variance"] == 3000.0
    assert body["treatment"] == "write_off_to_costing_profit_loss"


def test_records_and_analyses_persist_and_filter():
    _calculate()
    _calculate(cost_centre_id="CC-2", period="2026-12")
    client.post("/comprehensive-analysis", params=BASE, headers=H1)

    records = client.get("/records", headers=H1).json()["records"]
    assert len(records) == 2
    assert [
        r["cost_centre_id"] for r in client.get("/records", params={"period": "2026-12"}, headers=H1).json()["records"]
    ] == ["CC-2"]
    assert len(client.get("/analyses", headers=H1).json()["analyses"]) == 1


def test_records_are_caller_and_book_scoped():
    _calculate()
    client.post("/comprehensive-analysis", params=BASE, headers=H1)

    assert client.get("/records", headers=H2).json()["records"] == []
    assert client.get("/analyses", headers=H2).json()["analyses"] == []
    assert client.get("/records", headers=HB).json()["records"] == []


def test_caller_header_is_required_for_reads():
    assert client.get("/records").status_code == 422


def test_health_reports_the_service():
    body = client.get("/health").json()
    assert body["service"] == "over-under-absorption-service"
