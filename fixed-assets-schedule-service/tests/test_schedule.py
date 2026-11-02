"""Book-scoping and register tests for fixed-assets-schedule-service."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from fixed_assets_schedule_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("fas_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "fa-user-1", "fa-user-2"
BOOK_A, BOOK_B = "fa-book-a", "fa-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _register(code="VAN-1", cost=20000.0, accumulated=4000.0, category="motor_vehicles", headers=H1):
    return client.post(
        "/assets",
        json={
            "asset_code": code,
            "asset_name": f"Asset {code}",
            "category": category,
            "purchase_date": "2026-01-01T00:00:00",
            "purchase_cost": cost,
            "useful_life_years": 5,
            "accumulated_depreciation": accumulated,
        },
        headers=headers,
    )


class TestRegister:
    def test_a_new_asset_carries_cost_less_accumulated(self):
        body = _register().json()
        assert body["net_book_value"] == 16000.0
        assert body["status"] == "in_use"
        assert body["book_id"] == BOOK_A

    def test_a_supplied_book_value_is_ignored(self):
        response = client.post(
            "/assets",
            json={
                "asset_code": "X1",
                "asset_name": "Forced",
                "category": "land",
                "purchase_date": "2026-01-01T00:00:00",
                "purchase_cost": 10000,
                "useful_life_years": 10,
                "accumulated_depreciation": 1000,
                "net_book_value": 99999,
            },
            headers=H1,
        )
        assert response.json()["net_book_value"] == 9000.0

    def test_the_filters_read_their_flags(self):
        _register(code="VAN-1", category="motor_vehicles")
        _register(code="DESK-1", category="office_equipment", cost=2000.0, accumulated=0.0)
        by_cat = client.get("/assets", params={"category": "office_equipment"}, headers=H1).json()
        assert by_cat["count"] == 1
        assert by_cat["assets"][0]["asset_code"] == "DESK-1"
        assert client.get("/assets", params={"status": "disposed"}, headers=H1).json()["count"] == 0

    def test_another_book_sees_nothing(self):
        _register()
        assert client.get("/assets", headers=H1_BOOK_B).json()["count"] == 0


class TestUpdate:
    def test_an_update_writes_fields_but_never_recomputes_book_value(self):
        """The original left net book value where it was."""
        created = _register().json()
        body = client.put(
            f"/assets/{created['id']}",
            json={"status": "idle", "accumulated_depreciation": 9000.0},
            headers=H1,
        ).json()
        assert body["status"] == "idle"
        assert body["accumulated_depreciation"] == 9000.0
        assert body["net_book_value"] == 16000.0

    def test_the_id_and_created_at_survive_an_update(self):
        created = _register().json()
        body = client.put(
            f"/assets/{created['id']}",
            json={"id": "hijack", "created_at": "2000-01-01T00:00:00", "location": "Harare"},
            headers=H1,
        ).json()
        assert body["id"] == created["id"]
        assert body["created_at"] == created["created_at"]
        assert body["location"] == "Harare"

    def test_an_unknown_asset_is_a_404_on_read_and_write(self):
        assert client.get("/assets/ghost", headers=H1).status_code == 404
        assert client.put("/assets/ghost", json={"status": "idle"}, headers=H1).status_code == 404

    def test_another_user_cannot_read_or_write_it(self):
        created = _register().json()
        assert client.get(f"/assets/{created['id']}", headers=H2).status_code == 404
        assert client.put(f"/assets/{created['id']}", json={"status": "idle"}, headers=H2).status_code == 404
        assert client.get("/assets", headers=H1).json()["assets"][0]["status"] == "in_use"


class TestSchedule:
    def test_the_schedule_totals_and_categories(self):
        _register(code="VAN-1", category="motor_vehicles", cost=20000.0, accumulated=4000.0)
        _register(code="DESK-1", category="office_equipment", cost=2000.0, accumulated=0.0)
        body = client.post("/schedule/generate", params={"as_of_date": "2026-06-30T00:00:00"}, headers=H1).json()
        assert body["total_cost"] == 22000.0
        assert body["total_accumulated_depreciation"] == 4000.0
        assert body["total_net_book_value"] == 18000.0
        assert len(body["assets"]) == 2
        assert body["category_summary"]["motor_vehicles"]["nbv"] == 16000.0
        assert body["category_summary"]["office_equipment"]["count"] == 1

    def test_an_empty_category_is_left_out(self):
        _register(category="motor_vehicles")
        body = client.post("/schedule/generate", params={"as_of_date": "2026-06-30T00:00:00"}, headers=H1).json()
        assert list(body["category_summary"]) == ["motor_vehicles"]

    def test_latest_is_generated_on_demand(self):
        _register()
        first = client.get("/schedule/latest", headers=H1).json()
        second = client.get("/schedule/latest", headers=H1).json()
        assert first["id"] != second["id"]
        assert first["total_cost"] == 20000.0

    def test_another_users_schedule_is_empty(self):
        _register()
        body = client.post("/schedule/generate", params={"as_of_date": "2026-06-30T00:00:00"}, headers=H2).json()
        assert body["assets"] == []
        assert body["total_cost"] == 0

    def test_the_summary_reads_the_register(self):
        _register(code="VAN-1", cost=20000.0, accumulated=4000.0)
        _register(code="DESK-1", category="office_equipment", cost=2000.0, accumulated=0.0)
        body = client.get("/summary", headers=H1).json()
        assert body["total_assets"] == 2
        assert body["total_net_book_value"] == 18000.0
        assert body["by_category"]["motor_vehicles"]["cost"] == 20000.0
