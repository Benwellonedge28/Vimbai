"""Book-scoping and rate arithmetic tests for overhead-absorption-rate-service."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from overhead_absorption_rate_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("oar_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "oar-user-1", "oar-user-2"
BOOK_A, BOOK_B = "oar-book-a", "oar-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


class TestCalculate:
    def test_the_rate_divides_overhead_by_base(self):
        body = client.post(
            "/calculate",
            params={
                "cost_centre_name": "Machining",
                "cost_centre_id": "CC1",
                "budgeted_overhead": 60000.0,
                "budgeted_base_units": 3000.0,
            },
            headers=H1,
        ).json()
        assert body["absorption_rate"] == 20.0
        assert body["rate_per_unit"] == 20.0
        assert body["overhead_type"] == "production"
        assert body["absorption_base"] == "machine_hours"
        assert body["book_id"] == BOOK_A

    def test_a_zero_base_leaves_both_rates_at_zero(self):
        body = client.post(
            "/calculate",
            params={
                "cost_centre_name": "Idle",
                "cost_centre_id": "CC2",
                "budgeted_overhead": 5000.0,
                "budgeted_base_units": 0.0,
            },
            headers=H1,
        ).json()
        assert body["absorption_rate"] == 0
        assert body["rate_per_unit"] == 0

    def test_the_cost_centre_filter_reads_its_flag(self):
        for centre in ("CC1", "CC2"):
            client.post(
                "/calculate",
                params={
                    "cost_centre_name": centre,
                    "cost_centre_id": centre,
                    "budgeted_overhead": 1000.0,
                    "budgeted_base_units": 100.0,
                },
                headers=H1,
            )
        filtered = client.get("/rates", params={"cost_centre_id": "CC2"}, headers=H1).json()["rates"]
        assert [r["cost_centre_id"] for r in filtered] == ["CC2"]
        assert len(client.get("/rates", headers=H1).json()["rates"]) == 2

    def test_rates_are_book_scoped(self):
        client.post(
            "/calculate",
            params={
                "cost_centre_name": "Machining",
                "cost_centre_id": "CC1",
                "budgeted_overhead": 1000.0,
                "budgeted_base_units": 10.0,
            },
            headers=H1,
        )
        assert client.get("/rates", headers=H1_BOOK_B).json()["rates"] == []
        assert client.get("/analyses", headers=H1_BOOK_B).json()["analyses"] == []


class TestBases:
    def test_machine_hours_rate(self):
        body = client.post(
            "/machine-hours-rate",
            params={"cost_centre_name": "Machining", "budgeted_overhead": 9000.0, "budgeted_machine_hours": 1500.0},
            headers=H1,
        ).json()
        assert body["oar_per_machine_hour"] == 6.0
        assert body["formula"] == "9000.0 / 1500.0 = 6.0"

    def test_labour_hours_rate_has_no_formula(self):
        body = client.post(
            "/labour-hours-rate",
            params={
                "cost_centre_name": "Assembly",
                "budgeted_overhead": 8000.0,
                "budgeted_direct_labour_hours": 2000.0,
            },
            headers=H1,
        ).json()
        assert body["oar_per_direct_labour_hour"] == 4.0
        assert "formula" not in body

    def test_labour_cost_rate_is_a_percentage(self):
        body = client.post(
            "/labour-cost-rate",
            params={
                "cost_centre_name": "Assembly",
                "budgeted_overhead": 25000.0,
                "budgeted_direct_labour_cost": 100000.0,
            },
            headers=H1,
        ).json()
        assert body["oar_percentage"] == 25.0
        assert "%" in body["formula"]

    def test_material_and_prime_cost_rates(self):
        material = client.post(
            "/material-cost-rate",
            params={"cost_centre_name": "Stores", "budgeted_overhead": 5000.0, "budgeted_material_cost": 40000.0},
            headers=H1,
        ).json()
        assert material["oar_percentage"] == 12.5
        prime = client.post(
            "/prime-cost-rate",
            params={"cost_centre_name": "Stores", "budgeted_overhead": 3000.0, "budgeted_prime_cost": 12000.0},
            headers=H1,
        ).json()
        assert prime["oar_percentage"] == 25.0

    def test_a_zero_base_answers_zero_rather_than_dividing(self):
        body = client.post(
            "/material-cost-rate",
            params={"cost_centre_name": "Stores", "budgeted_overhead": 5000.0, "budgeted_material_cost": 0.0},
            headers=H1,
        ).json()
        assert body["oar_percentage"] == 0

    def test_unit_rate(self):
        body = client.post(
            "/unit-rate",
            params={"cost_centre_name": "Packing", "budgeted_overhead": 1200.0, "budgeted_units": 400.0},
            headers=H1,
        ).json()
        assert body["oar_per_unit"] == 3.0


class TestBlanketAndAbsorbed:
    def test_a_blanket_rate_is_named_for_the_whole_factory(self):
        body = client.post(
            "/blanket-rate",
            params={
                "entity_name": "Factory",
                "total_budgeted_overhead": 500000.0,
                "total_base_units": 25000.0,
                "absorption_base": "machine_hours",
            },
            headers=H1,
        ).json()
        assert body["blanket_oar"] == 20.0
        analysis = client.get("/analyses", headers=H1).json()["analyses"][0]
        assert analysis["cost_centre_name"] == "entire_factory"
        assert analysis["rate_type"] == "blanket"

    def test_absorbed_overhead_is_not_recorded(self):
        body = client.post(
            "/absorbed-overhead",
            params={"cost_centre_name": "Machining", "actual_base_units": 1400.0, "absorption_rate": 6.0},
            headers=H1,
        ).json()
        assert body["overhead_absorbed"] == 8400.0
        assert client.get("/analyses", headers=H1).json()["analyses"] == []


class TestAnalyses:
    def test_an_analysis_records_the_budget_working(self):
        client.post(
            "/machine-hours-rate",
            params={"cost_centre_name": "Machining", "budgeted_overhead": 9000.0, "budgeted_machine_hours": 1500.0},
            headers=H1,
        )
        analysis = client.get("/analyses", headers=H1).json()["analyses"][0]
        assert analysis["rate_type"] == "departmental"
        assert analysis["calculated_rate"] == 6.0
        assert analysis["base_units_used"] == 1500.0
        assert analysis["overhead_absorbed"] == 9000.0

    def test_analyses_are_book_scoped(self):
        client.post(
            "/labour-hours-rate",
            params={"cost_centre_name": "A", "budgeted_overhead": 100.0, "budgeted_direct_labour_hours": 10.0},
            headers=H1,
        )
        assert client.get("/analyses", headers=H2).json()["analyses"] == []
        assert len(client.get("/analyses", headers=H1).json()["analyses"]) == 1
