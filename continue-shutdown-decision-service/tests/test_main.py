"""Book-scoping, persistence and arithmetic tests for continue-shutdown-decision-service.

Covers the continue-versus-shutdown comparison, the contribution test, the
stateless quick analysis, listing filters and lookup (including the original
200-with-error contract), and cross-user / cross-Book isolation.

Everything decision used to sit in a process-global list shared by every tenant;
it is now grep-able only through the caller's own Book.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from continue_shutdown_decision_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("csd_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "csd-user-1", "csd-user-2"
BOOK_A, BOOK_B = "csd-book-a", "csd-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


BASE = {
    "entity_id": "DEPT-1",
    "entity_name": "Assembly",
    "entity_type": "department",
    "period": "2026-11",
    "revenue": 500000.0,
    "variable_costs": 300000.0,
    "avoidable_fixed_costs": 150000.0,
    "unavoidable_fixed_costs": 100000.0,
}


def _analyze(headers=H1, **overrides):
    params = dict(BASE)
    params.update(overrides)
    return client.post("/analyze", params=params, headers=headers).json()


def test_continue_wins_when_contribution_covers_unavoidable_costs():
    body = _analyze()
    assert body["cost_analysis"]["contribution"] == 200000.0
    assert body["contribution_if_continue"] == 200000.0
    assert body["financial_outcome_continue"] == 100000.0
    assert body["net_shutdown_benefit"] == 150000.0
    assert body["financial_outcome_shutdown"] == 50000.0
    assert body["recommendation"] == "CONTINUE"


def test_shutdown_wins_when_contribution_is_below_unavoidable_costs():
    body = _analyze(revenue=350000.0)
    assert body["financial_outcome_continue"] == -50000.0
    assert body["financial_outcome_shutdown"] == 50000.0
    assert body["recommendation"] == "SHUTDOWN"


def test_shutdown_costs_and_savings_move_the_benefit():
    body = _analyze(shutdown_costs=40000.0, shutdown_savings=10000.0)
    assert body["net_shutdown_benefit"] == 120000.0
    assert body["financial_outcome_shutdown"] == 20000.0
    assert body["recommendation"] == "CONTINUE"


def test_equal_outcomes_are_indifferent():
    # shutdown = -100000 + 200000 avoidable saved; continue = 200000 - 100000
    body = _analyze(avoidable_fixed_costs=200000.0)
    assert body["financial_outcome_continue"] == 100000.0
    assert body["financial_outcome_shutdown"] == 100000.0
    assert body["recommendation"] == "INDIFFERENT"


def test_quick_analysis_is_stateless():
    body = client.post(
        "/quick-analysis",
        params={"entity_name": "Assembly", "contribution": 200000.0, "unavoidable_fixed_costs": 100000.0},
        headers=H1,
    ).json()
    assert body["recommendation"] == "CONTINUE"
    assert body["financial_outcome"] == 100000.0
    assert client.get("/decisions", headers=H1).json()["decisions"] == []


def test_decisions_persist_and_filter():
    created = _analyze()
    _analyze(entity_id="DEPT-2", entity_type="product")

    assert len(client.get("/decisions", headers=H1).json()["decisions"]) == 2
    assert len(client.get("/decisions", params={"entity_type": "product"}, headers=H1).json()["decisions"]) == 1
    fetched = client.get(f"/decisions/{created['id']}", headers=H1).json()
    assert fetched["recommendation"] == "CONTINUE"
    assert fetched["cost_analysis"]["unavoidable_fixed_costs"] == 100000.0


def test_unknown_decision_keeps_the_original_error_body():
    assert client.get("/decisions/nope", headers=H1).json() == {"error": "Decision not found"}


def test_decisions_are_caller_and_book_scoped():
    created = _analyze()
    assert client.get("/decisions", headers=H2).json()["decisions"] == []
    assert client.get("/decisions", headers=HB).json()["decisions"] == []
    assert client.get(f"/decisions/{created['id']}", headers=H2).json() == {"error": "Decision not found"}


def test_caller_header_is_required_for_reads():
    assert client.get("/decisions").status_code == 422


def test_health_reports_the_service():
    body = client.get("/health").json()
    assert body["service"] == "continue-shutdown-decision-service"
