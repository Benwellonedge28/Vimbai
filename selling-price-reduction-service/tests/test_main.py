"""Book-scoping, persistence and arithmetic tests for selling-price-reduction-service.

Covers the contribution move at the proposed price, the substitution volume needed
to hold contribution, the recommendation ladder, the stateless discount ladder,
listing filters, and cross-user / cross-Book isolation.

Everything analysis used to sit in a process-global list shared by every tenant;
it is now grep-able only through the caller's own Book.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from fastapi.testclient import TestClient
from selling_price_reduction_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("spr_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "spr-user-1", "spr-user-2"
BOOK_A, BOOK_B = "spr-book-a", "spr-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


ANALYZE = {
    "product_id": "PRD-1",
    "product_name": "Widget",
    "current_price": 100.0,
    "proposed_price": 90.0,
    "variable_cost_per_unit": 60.0,
    "current_expected_sales": 1000.0,
}


def _analyze(headers=H1, **overrides):
    params = dict(ANALYZE)
    params.update(overrides)
    return client.post("/analyze", params=params, headers=headers).json()


def test_reduction_moves_contribution_per_unit():
    body = _analyze()
    assert body["price_reduction"] == 10.0
    assert body["price_reduction_percentage"] == 10.0
    assert body["current_contribution_per_unit"] == 40.0
    assert body["new_contribution_per_unit"] == 30.0
    assert body["contribution_change_per_unit"] == -10.0
    assert body["current_total_contribution"] == 40000.0
    assert body["new_total_contribution"] == 30000.0
    assert body["contribution_difference"] == -10000.0


def test_volume_needed_to_hold_contribution():
    body = _analyze()
    # 40000 of contribution at 30 per unit needs 1333.33 units
    assert body["break_even_volume_increase"] == pytest.approx(40000.0 / 30.0 - 1000.0)


def test_shortfall_without_announced_volume_is_conditional():
    # no substitution volume supplied: the answer is conditional, not a rejection
    assert _analyze()["recommendation"] == "CONDITIONAL_APPROVAL"


def test_announced_volume_that_still_falls_short_is_rejected():
    # 1200 units at 30 still beats nothing back against 40000 of contribution
    assert _analyze(new_expected_sales=1200.0)["recommendation"] == "REJECT"


def test_volume_growth_can_approve_the_reduction():
    body = _analyze(new_expected_sales=1500.0)
    assert body["new_total_contribution"] == 45000.0
    assert body["recommendation"] == "APPROVE"


def test_target_contribution_sets_the_price_floor():
    body = _analyze(target_contribution=50000.0)
    # 50000 over 1000 units = 50 per unit on top of 60 variable cost
    assert body["minimum_price_for_target_contribution"] == 110.0


def test_discount_ladder_is_stateless():
    body = client.post(
        "/discount-analysis",
        params={
            "product_name": "Widget",
            "list_price": 100.0,
            "variable_cost_per_unit": 60.0,
            "expected_sales_at_list": 1000.0,
        },
        json=[5, 10],
        headers=H1,
    )
    assert body.status_code == 200, body.text
    served = body.json()["discount_analysis"]
    assert [r["discount_percentage"] for r in served] == [5, 10]
    assert served[0]["selling_price"] == pytest.approx(95.0)
    assert served[0]["estimated_sales_volume"] == pytest.approx(1100.0)
    # 5% off: price 95, contribution 35, volume up 10% to 1100 -> 38500 vs 40000
    assert served[0]["total_contribution"] == pytest.approx(38500.0)
    assert served[0]["contribution_vs_original"] == pytest.approx(-1500.0)
    assert served[0]["recommended"] is False
    assert client.get("/analyses", headers=H1).json()["analyses"] == []


def test_analyses_persist_and_filter():
    _analyze()
    _analyze(product_id="PRD-2")
    assert len(client.get("/analyses", headers=H1).json()["analyses"]) == 2
    assert len(client.get("/analyses", params={"product_id": "PRD-2"}, headers=H1).json()["analyses"]) == 1


def test_analyses_are_caller_and_book_scoped():
    _analyze()
    assert client.get("/analyses", headers=H2).json()["analyses"] == []
    assert client.get("/analyses", headers=HB).json()["analyses"] == []


def test_caller_header_is_required_for_reads():
    assert client.get("/analyses").status_code == 422


def test_health_reports_the_service():
    body = client.get("/health").json()
    assert body["service"] == "selling-price-reduction-service"
