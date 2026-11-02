"""Book-scoping and amortisation tests for intangible-assets-service."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from intangible_assets_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ia_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "ia-user-1", "ia-user-2"
BOOK_A, BOOK_B = "ia-book-a", "ia-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _create(
    name="Patent A", asset_type="patent", cost=120000.0, life=10, residual=0.0, method="straight_line", headers=H1
):
    return client.post(
        "/assets",
        params={
            "name": name,
            "asset_type": asset_type,
            "cost": cost,
            "useful_life_years": life,
            "acquisition_date": "2026-01-01T00:00:00",
            "residual_value": residual,
            "amortization_method": method,
        },
        headers=headers,
    )


class TestRegister:
    def test_an_asset_is_registered_at_full_cost(self):
        body = _create(residual=20000.0).json()
        assert body["net_book_value"] == 120000.0
        assert body["residual_value"] == 20000.0
        assert body["status"] == "active"
        assert body["book_id"] == BOOK_A

    def test_an_unknown_type_is_rejected(self):
        response = _create(asset_type="vibes")
        assert response.status_code == 400
        assert "Invalid type" in response.json()["detail"]

    def test_the_filters_read_their_flags(self):
        _create(name="Patent A", asset_type="patent")
        _create(name="Licence B", asset_type="license")
        listed = client.get("/assets", params={"asset_type": "license"}, headers=H1).json()
        assert [a["name"] for a in listed] == ["Licence B"]
        assert client.get("/assets", params={"status": "disposed"}, headers=H1).json() == []

    def test_another_book_sees_nothing(self):
        created = _create().json()
        assert client.get("/assets", headers=H1_BOOK_B).json() == []
        assert client.get(f"/assets/{created['id']}", headers=H1_BOOK_B).status_code == 404


class TestAmortisation:
    def test_straight_line_spreads_cost_over_the_life(self):
        created = _create(cost=120000.0, life=10).json()
        entry = client.post(f"/assets/{created['id']}/amortize", params={"period": "2026-01"}, headers=H1).json()
        # (120000 - 0) / 10 / 12
        assert entry["amortization_amount"] == 1000.0
        assert entry["accumulated_amortization"] == 1000.0
        assert entry["net_book_value"] == 119000.0

    def test_the_residual_value_is_excluded_from_the_base(self):
        created = _create(cost=120000.0, life=10, residual=24000.0).json()
        entry = client.post(f"/assets/{created['id']}/amortize", params={"period": "2026-01"}, headers=H1).json()
        assert entry["amortization_amount"] == 800.0

    def test_reducing_balance_takes_ten_percent_of_the_excess(self):
        created = _create(cost=120000.0, method="reducing_balance", residual=0.0).json()
        entry = client.post(f"/assets/{created['id']}/amortize", params={"period": "2026-01"}, headers=H1).json()
        # 120000 * 0.1 / 12
        assert entry["amortization_amount"] == 1000.0

    def test_the_accumulated_figure_overshoots_while_the_book_value_is_clamped(self):
        """The original never caps the accumulated figure, only the book value."""
        created = _create(cost=12000.0, life=1, residual=1000.0).json()
        for month in range(13):
            last = client.post(
                f"/assets/{created['id']}/amortize",
                params={"period": f"2026-{month + 1:02d}"},
                headers=H1,
            ).json()
        # 916.67 a month for thirteen months, while the book value stops at residual
        assert last["amortization_amount"] == pytest.approx(916.666, rel=1e-4)
        assert last["accumulated_amortization"] > 11000.0
        assert last["net_book_value"] == 1000.0
        stored = client.get(f"/assets/{created['id']}", headers=H1).json()
        assert stored["status"] == "fully_amortized"

    def test_a_fully_amortised_asset_refuses_further_charges(self):
        created = _create(cost=12000.0, life=1).json()
        for month in range(12):
            client.post(f"/assets/{created['id']}/amortize", params={"period": f"2026-{month + 1:02d}"}, headers=H1)
        response = client.post(f"/assets/{created['id']}/amortize", params={"period": "2027-01"}, headers=H1)
        assert response.status_code == 400
        assert response.json()["detail"] == "Asset is fully_amortized, cannot amortize"

    def test_the_history_lists_in_order(self):
        created = _create().json()
        for month in ("2026-01", "2026-02"):
            client.post(f"/assets/{created['id']}/amortize", params={"period": month}, headers=H1)
        history = client.get(f"/assets/{created['id']}/amortization", headers=H1).json()
        assert [e["period"] for e in history] == ["2026-01", "2026-02"]

    def test_another_user_gets_no_history_and_cannot_charge(self):
        created = _create().json()
        client.post(f"/assets/{created['id']}/amortize", params={"period": "2026-01"}, headers=H1)
        assert client.get(f"/assets/{created['id']}/amortization", headers=H2).json() == []
        response = client.post(f"/assets/{created['id']}/amortize", params={"period": "2026-02"}, headers=H2)
        assert response.status_code == 404
        # the owner's asset is untouched
        stored = client.get(f"/assets/{created['id']}", headers=H1).json()
        assert stored["accumulated_amortization"] == 1000.0
