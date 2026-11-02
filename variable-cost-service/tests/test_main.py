"""Book-scoping and persistence tests for variable-cost-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from variable_cost_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("variable_cost_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "vc-user-1", "vc-user-2"
BOOK_A, BOOK_B = "vc-book-a", "vc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _add(unit_cost=4.0, quantity=100.0, code="MAT", driver="units", product=None, headers=H1):
    params = {
        "cost_name": code.title(),
        "cost_code": code,
        "unit_cost": unit_cost,
        "quantity": quantity,
        "cost_driver": driver,
    }
    if product:
        params["product_id"] = product
    return client.post("/costs/add", params=params, headers=headers)


class TestCosts:
    def test_a_cost_arrives_with_its_total(self):
        created = _add().json()
        assert created["total_cost"] == 400.0
        # the driver rate starts as the unit cost
        assert created["cost_driver_rate"] == 4.0
        assert created["book_id"] == BOOK_A
        listed = client.get("/costs", headers=H1).json()["variable_costs"]
        assert [c["id"] for c in listed] == [created["id"]]

    def test_update_recomputes_the_total(self):
        created = _add().json()
        updated = client.post(f"/costs/{created['id']}/update", params={"quantity": 150.0}, headers=H1).json()
        assert updated["total_cost"] == 600.0
        # the driver rate is not touched by an update
        assert updated["cost_driver_rate"] == 4.0
        assert client.get("/costs", headers=H1).json()["variable_costs"][0]["total_cost"] == 600.0

    def test_an_unknown_cost_reads_as_not_found(self):
        assert client.post("/costs/nope/update", params={"quantity": 1}, headers=H1).json() == {
            "error": "Cost not found"
        }

    def test_another_user_cannot_touch_it(self):
        created = _add().json()
        assert client.post(f"/costs/{created['id']}/update", params={"quantity": 1}, headers=H2).json() == {
            "error": "Cost not found"
        }
        assert client.get("/costs", headers=H2).json()["variable_costs"] == []

    def test_another_book_cannot_see_it(self):
        _add()
        assert client.get("/costs", headers=H1_BOOK_B).json()["variable_costs"] == []
        assert client.get("/summary", headers=H1_BOOK_B).json()["total_cost_items"] == 0


class TestAnalysis:
    def test_the_analysis_groups_by_driver_and_product(self):
        _add(unit_cost=4.0, quantity=100.0, code="MAT", driver="units", product="p1")
        _add(unit_cost=10.0, quantity=5.0, code="LAB", driver="hours", product="p2")
        analysis = client.post("/analyze", params={"analysis_period": "2026-10"}, headers=H1).json()
        assert analysis["total_variable_costs"] == 450.0
        assert analysis["total_units"] == 105.0
        assert round(analysis["average_cost_per_unit"], 4) == round(450.0 / 105.0, 4)
        assert analysis["cost_driver_analysis"] == {"units": 400.0, "hours": 50.0}
        assert analysis["product_costs"] == {"p1": 400.0, "p2": 50.0}

    def test_an_analysis_without_units_leaves_the_average_at_zero(self):
        analysis = client.post("/analyze", params={"analysis_period": "2026-10"}, headers=H1).json()
        assert analysis["average_cost_per_unit"] == 0
        assert analysis["total_variable_costs"] == 0

    def test_only_the_callers_costs_are_counted(self):
        _add()
        _add(unit_cost=100.0, quantity=10.0, code="OTHER", headers=H2)
        analysis = client.post("/analyze", params={"analysis_period": "2026-10"}, headers=H1).json()
        assert analysis["total_variable_costs"] == 400.0


class TestSummary:
    def test_the_summary_lists_the_drivers_in_use(self):
        _add(unit_cost=4.0, quantity=100.0, code="MAT", driver="units")
        _add(unit_cost=10.0, quantity=5.0, code="LAB", driver="hours")
        summary = client.get("/summary", headers=H1).json()
        assert summary["total_variable_costs"] == 450.0
        assert summary["total_units"] == 105.0
        assert sorted(summary["cost_drivers"]) == ["hours", "units"]
        assert summary["total_cost_items"] == 2

    def test_the_calculator_stays_pure(self):
        body = client.post("/calculate-total", params={"unit_cost": 7.5, "expected_quantity": 200}).json()
        assert body["total_variable_cost"] == 1500.0
        assert body["cost_per_driver_unit"] == 7.5
        # nothing is stored by it
        assert client.get("/costs", headers=H1).json()["variable_costs"] == []
