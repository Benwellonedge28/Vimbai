"""Book-scoping and basis arithmetic tests for basis-apportionment-service."""

import importlib.util
import os

import main
import pytest
from basis_apportionment_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ba_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "ba-user-1", "ba-user-2"
BOOK_A, BOOK_B = "ba-book-a", "ba-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

CENTRES = [
    {"cost_centre_id": "CC1", "cost_centre_name": "Machining", "floor_area": 600},
    {"cost_centre_id": "CC2", "cost_centre_name": "Assembly", "floor_area": 300},
    {"cost_centre_id": "CC3", "cost_centre_name": "Stores", "floor_area": 100},
]


def _apportion(path="floor-area", total=10000.0, centres=None, cost_type="rent", headers=H1):
    return client.post(
        f"/apportion/{path}",
        params={"cost_type": cost_type, "total_cost": total},
        json=centres if centres is not None else CENTRES,
        headers=headers,
    )


class TestRules:
    def test_a_rule_is_recorded(self):
        body = client.post(
            "/rules/create",
            params={"cost_type": "rent", "basis": "floor_area", "description": "Rent by area"},
            headers=H1,
        ).json()
        assert body["basis"] == "floor_area"
        assert body["book_id"] == BOOK_A
        assert len(client.get("/rules", headers=H1).json()["rules"]) == 1

    def test_rules_are_book_scoped(self):
        client.post(
            "/rules/create",
            params={"cost_type": "rent", "basis": "floor_area", "description": "Rent"},
            headers=H1,
        )
        assert client.get("/rules", headers=H1_BOOK_B).json()["rules"] == []


class TestFloorArea:
    def test_proportions_and_totals(self):
        body = _apportion().json()
        proportions = [c["proportion"] for c in body["cost_centres"]]
        assert proportions == [0.6, 0.3, 0.1]
        assert body["cost_centres"][0]["apportioned_cost"] == 6000.0
        assert body["total_apportioned"] == 10000.0
        assert body["unapportioned"] == 0.0
        assert body["basis"] == "floor_area"

    def test_a_zero_basis_apportions_nothing(self):
        zeroed = [dict(c, floor_area=0) for c in CENTRES]
        body = _apportion(centres=zeroed).json()
        assert body["cost_centres"] == []
        assert body["total_apportioned"] == 0
        assert body["unapportioned"] == 10000.0

    def test_a_centre_without_the_basis_counts_as_zero_but_still_appears(self):
        centres = [
            {"cost_centre_id": "CC1", "cost_centre_name": "Machining", "floor_area": 400},
            {"cost_centre_id": "CC2", "cost_centre_name": "Assembly"},
        ]
        body = _apportion(centres=centres).json()
        assert [c["proportion"] for c in body["cost_centres"]] == [1.0, 0.0]
        assert body["cost_centres"][1]["apportioned_cost"] == 0.0

    def test_another_book_apportions_separately(self):
        _apportion()
        assert client.get("/results", headers=H1_BOOK_B).json()["results"] == []


class TestEachBasis:
    def test_volume(self):
        centres = [
            {"cost_centre_id": "CC1", "cost_centre_name": "A", "volume": 250},
            {"cost_centre_id": "CC2", "cost_centre_name": "B", "volume": 750},
        ]
        body = _apportion("volume", centres=centres).json()
        assert [c["proportion"] for c in body["cost_centres"]] == [0.25, 0.75]
        assert body["basis"] == "volume"

    def test_cost_reads_the_asset_cost_key(self):
        """The cost basis reads "asset_cost" on the rows, not "cost"."""
        centres = [
            {"cost_centre_id": "CC1", "cost_centre_name": "A", "asset_cost": 90000},
            {"cost_centre_id": "CC2", "cost_centre_name": "B", "asset_cost": 10000},
        ]
        body = _apportion("cost", centres=centres).json()
        assert body["basis"] == "cost"
        assert [c["proportion"] for c in body["cost_centres"]] == [0.9, 0.1]

    def test_cost_ignores_a_row_that_only_supplies_cost(self):
        centres = [{"cost_centre_id": "CC1", "cost_centre_name": "A", "cost": 500}]
        body = _apportion("cost", centres=centres).json()
        assert body["cost_centres"] == []
        assert body["unapportioned"] == 10000.0

    def test_net_book_value(self):
        centres = [
            {"cost_centre_id": "CC1", "cost_centre_name": "A", "net_book_value": 30000},
            {"cost_centre_id": "CC2", "cost_centre_name": "B", "net_book_value": 10000},
        ]
        body = _apportion("net-book-value", centres=centres).json()
        assert [c["proportion"] for c in body["cost_centres"]] == [0.75, 0.25]

    def test_replacement_value(self):
        centres = [
            {"cost_centre_id": "CC1", "cost_centre_name": "A", "replacement_value": 50000},
            {"cost_centre_id": "CC2", "cost_centre_name": "B", "replacement_value": 50000},
        ]
        body = _apportion("replacement-value", centres=centres).json()
        assert [c["proportion"] for c in body["cost_centres"]] == [0.5, 0.5]

    def test_personnel(self):
        centres = [
            {"cost_centre_id": "CC1", "cost_centre_name": "A", "number_of_personnel": 30},
            {"cost_centre_id": "CC2", "cost_centre_name": "B", "number_of_personnel": 10},
        ]
        body = _apportion("personnel", centres=centres).json()
        assert body["basis"] == "number_of_personnel"
        assert [c["proportion"] for c in body["cost_centres"]] == [0.75, 0.25]

    def test_requisitions(self):
        centres = [
            {"cost_centre_id": "CC1", "cost_centre_name": "A", "number_of_requisitions": 4},
            {"cost_centre_id": "CC2", "cost_centre_name": "B", "number_of_requisitions": 1},
        ]
        body = _apportion("requisitions", centres=centres).json()
        assert body["basis"] == "number_of_requisitions"
        assert [c["apportioned_cost"] for c in body["cost_centres"]] == [8000.0, 2000.0]


class TestResults:
    def test_results_accumulate_and_filter(self):
        _apportion("floor-area", cost_type="rent")
        _apportion(
            "volume", cost_type="insurance", centres=[{"cost_centre_id": "CC1", "cost_centre_name": "A", "volume": 5}]
        )
        every = client.get("/results", headers=H1).json()["results"]
        assert len(every) == 2
        filtered = client.get("/results", params={"cost_type": "insurance"}, headers=H1).json()["results"]
        assert [r["basis"] for r in filtered] == ["volume"]

    def test_a_result_keeps_its_breakdown(self):
        _apportion()
        stored = client.get("/results", headers=H1).json()["results"][0]
        assert [c["cost_centre_name"] for c in stored["cost_centres"]] == ["Machining", "Assembly", "Stores"]

    def test_another_user_cannot_read_the_results(self):
        _apportion()
        assert client.get("/results", headers=H2).json()["results"] == []
