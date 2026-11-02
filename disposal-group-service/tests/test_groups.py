"""Book-scoping and IFRS 5 tests for disposal-group-service."""

import importlib.util
import os

import main
import pytest
from disposal_group_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("dg_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "dg-user-1", "dg-user-2"
BOOK_A, BOOK_B = "dg-book-a", "dg-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _group(name="Plant sale", headers=H1):
    return client.post("/groups", params={"name": name}, headers=headers)


def _add(group_id, asset_id="A1", carrying=100000.0, fair=90000.0, headers=H1):
    return client.post(
        f"/groups/{group_id}/assets",
        params={
            "asset_id": asset_id,
            "asset_name": f"Asset {asset_id}",
            "carrying_amount": carrying,
            "fair_value": fair,
        },
        headers=headers,
    )


class TestGroups:
    def test_a_group_starts_held_for_sale(self):
        body = _group().json()
        assert body["status"] == "held_for_sale"
        assert body["disposal_method"] == "sale"
        assert body["total_carrying_amount"] == 0.0
        assert body["book_id"] == BOOK_A

    def test_the_status_filter_reads_its_flag(self):
        first = _group(name="One").json()
        _group(name="Two")
        client.put(f"/groups/{first['id']}/status", params={"status": "sold"}, headers=H1)
        sold = client.get("/groups", params={"status": "sold"}, headers=H1).json()
        assert [g["name"] for g in sold] == ["One"]
        assert len(client.get("/groups", headers=H1).json()) == 2

    def test_another_book_sees_no_groups(self):
        _group()
        assert client.get("/groups", headers=H1_BOOK_B).json() == []


class TestAssets:
    def test_assets_roll_into_the_group_totals(self):
        group = _group().json()
        _add(group["id"], "A1", carrying=100000.0, fair=90000.0)
        _add(group["id"], "A2", carrying=50000.0, fair=55000.0)
        listed = client.get("/groups", headers=H1).json()[0]
        assert listed["total_carrying_amount"] == 150000.0
        assert listed["total_fair_value"] == 145000.0
        assets = client.get(f"/groups/{group['id']}/assets", headers=H1).json()
        assert len(assets) == 2

    def test_assets_come_back_in_carrying_order(self):
        group = _group().json()
        _add(group["id"], "BIG", carrying=100000.0)
        _add(group["id"], "SMALL", carrying=1000.0)
        assets = client.get(f"/groups/{group['id']}/assets", headers=H1).json()
        assert [a["asset_id"] for a in assets] == ["SMALL", "BIG"]

    def test_an_unknown_group_cannot_take_assets(self):
        response = _add("ghost")
        assert response.status_code == 404
        assert response.json()["detail"] == "Disposal group not found"

    def test_listing_assets_of_an_unknown_group_is_empty_not_a_404(self):
        assert client.get("/groups/ghost/assets", headers=H1).json() == []

    def test_another_user_cannot_add_or_read_assets(self):
        group = _group().json()
        _add(group["id"], "A1")
        assert _add(group["id"], "A2", headers=H2).status_code == 404
        assert client.get(f"/groups/{group['id']}/assets", headers=H2).json() == []
        assert len(client.get(f"/groups/{group['id']}/assets", headers=H1).json()) == 1


class TestImpairment:
    def test_impairment_is_carrying_less_proceeds(self):
        group = _group().json()
        _add(group["id"], carrying=100000.0, fair=90000.0)
        _add(group["id"], carrying=50000.0, fair=55000.0)
        test = client.post(
            f"/groups/{group['id']}/impairment-test",
            params={"fair_value_less_costs": 120000.0},
            headers=H1,
        ).json()
        assert test["carrying_amount"] == 150000.0
        assert test["impairment_amount"] == 30000.0
        assert client.get("/groups", headers=H1).json()[0]["impairment_loss"] == 30000.0

    def test_proceeds_above_carrying_means_no_impairment(self):
        group = _group().json()
        _add(group["id"], carrying=100000.0)
        test = client.post(
            f"/groups/{group['id']}/impairment-test",
            params={"fair_value_less_costs": 130000.0},
            headers=H1,
        ).json()
        assert test["impairment_amount"] == 0

    def test_a_later_test_replaces_the_recorded_loss(self):
        group = _group().json()
        _add(group["id"], carrying=100000.0)
        client.post(
            f"/groups/{group['id']}/impairment-test",
            params={"fair_value_less_costs": 70000.0},
            headers=H1,
        )
        client.post(
            f"/groups/{group['id']}/impairment-test",
            params={"fair_value_less_costs": 95000.0},
            headers=H1,
        )
        assert client.get("/groups", headers=H1).json()[0]["impairment_loss"] == 5000.0

    def test_an_unknown_group_cannot_be_tested(self):
        response = client.post("/groups/ghost/impairment-test", params={"fair_value_less_costs": 1.0}, headers=H1)
        assert response.status_code == 404


class TestStatus:
    def test_a_valid_status_is_accepted(self):
        group = _group().json()
        body = client.put(f"/groups/{group['id']}/status", params={"status": "sold"}, headers=H1).json()
        assert body == {"group_id": group["id"], "status": "sold"}
        assert client.get("/groups", headers=H1).json()[0]["status"] == "sold"

    def test_an_invalid_status_is_rejected_before_the_lookup(self):
        """The original checked the status first, so an unknown group answers 400."""
        group = _group().json()
        response = client.put(f"/groups/{group['id']}/status", params={"status": "donated"}, headers=H1)
        assert response.status_code == 400
        response = client.put("/groups/ghost/status", params={"status": "donated"}, headers=H1)
        assert response.status_code == 400

    def test_an_unknown_group_with_a_valid_status_is_a_404(self):
        assert client.put("/groups/ghost/status", params={"status": "sold"}, headers=H1).status_code == 404

    def test_another_user_cannot_change_the_status(self):
        group = _group().json()
        assert client.put(f"/groups/{group['id']}/status", params={"status": "sold"}, headers=H2).status_code == 404
        assert client.get("/groups", headers=H1).json()[0]["status"] == "held_for_sale"
