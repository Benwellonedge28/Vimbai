"""Book-scoping and valuation tests for investment-monitoring-service."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from investment_monitoring_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("im_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "im-user-1", "im-user-2"
BOOK_A, BOOK_B = "im-book-a", "im-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _portfolio(name="Balanced Fund", headers=H1):
    return client.post("/portfolios", params={"name": name}, headers=headers)


def _holding(portfolio_id, name="Acme Corp", quantity=100.0, cost=50.0, price=60.0, headers=H1):
    return client.post(
        f"/portfolios/{portfolio_id}/holdings",
        params={
            "instrument_name": name,
            "instrument_type": "equity",
            "quantity": quantity,
            "purchase_price": cost,
            "current_price": price,
        },
        headers=headers,
    )


class TestPortfolios:
    def test_a_portfolio_starts_moderate(self):
        body = _portfolio().json()
        assert body["risk_tolerance"] == "moderate"
        assert body["target_return"] == 0.0
        assert body["book_id"] == BOOK_A

    def test_the_listing_is_book_scoped(self):
        _portfolio("One")
        _portfolio("Two")
        assert len(client.get("/portfolios", headers=H1).json()) == 2
        assert client.get("/portfolios", headers=H1_BOOK_B).json() == []
        assert client.get("/portfolios", headers=H2).json() == []


class TestHoldings:
    def test_a_holding_lands_in_its_portfolio(self):
        portfolio = _portfolio().json()
        body = _holding(portfolio["id"]).json()
        assert body["portfolio_id"] == portfolio["id"]
        assert body["currency"] == "USD"
        listed = client.get(f"/portfolios/{portfolio['id']}/holdings", headers=H1).json()
        assert [h["instrument_name"] for h in listed] == ["Acme Corp"]

    def test_an_unknown_portfolio_cannot_take_holdings(self):
        response = _holding("ghost")
        assert response.status_code == 404
        assert response.json()["detail"] == "Portfolio not found"

    def test_listing_holdings_of_an_unknown_portfolio_is_empty(self):
        assert client.get("/portfolios/ghost/holdings", headers=H1).json() == []

    def test_another_user_cannot_add_or_see_holdings(self):
        portfolio = _portfolio().json()
        _holding(portfolio["id"])
        assert _holding(portfolio["id"], name="Intruder", headers=H2).status_code == 404
        assert client.get(f"/portfolios/{portfolio['id']}/holdings", headers=H2).json() == []
        assert len(client.get(f"/portfolios/{portfolio['id']}/holdings", headers=H1).json()) == 1


class TestPerformance:
    def test_valuation_and_unrealised_gain(self):
        portfolio = _portfolio().json()
        _holding(portfolio["id"], quantity=100.0, cost=50.0, price=60.0)
        _holding(portfolio["id"], name="Bond X", quantity=200.0, cost=10.0, price=12.0)
        body = client.post(f"/portfolios/{portfolio['id']}/performance", headers=H1).json()
        assert body["total_value"] == 8400.0
        assert body["total_cost"] == 7000.0
        assert body["unrealized_gain"] == 1400.0
        assert body["unrealized_gain_pct"] == 20.0

    def test_a_loss_reads_as_a_negative_gain(self):
        portfolio = _portfolio().json()
        _holding(portfolio["id"], quantity=10.0, cost=100.0, price=80.0)
        body = client.post(f"/portfolios/{portfolio['id']}/performance", headers=H1).json()
        assert body["unrealized_gain"] == -200.0
        assert body["unrealized_gain_pct"] == -20.0

    def test_an_empty_portfolio_is_a_404_naming_holdings(self):
        portfolio = _portfolio().json()
        response = client.post(f"/portfolios/{portfolio['id']}/performance", headers=H1)
        assert response.status_code == 404
        assert response.json()["detail"] == "No holdings found for portfolio"

    def test_another_users_portfolio_has_no_holdings_to_value(self):
        portfolio = _portfolio().json()
        _holding(portfolio["id"])
        assert client.post(f"/portfolios/{portfolio['id']}/performance", headers=H2).status_code == 404

    def test_the_listing_returns_the_tail_up_to_the_limit(self):
        portfolio = _portfolio().json()
        _holding(portfolio["id"])
        for _ in range(3):
            client.post(f"/portfolios/{portfolio['id']}/performance", headers=H1)
        all_metrics = client.get(f"/portfolios/{portfolio['id']}/performance", headers=H1).json()
        assert len(all_metrics) == 3
        tail = client.get(f"/portfolios/{portfolio['id']}/performance", params={"limit": 2}, headers=H1).json()
        assert [m["id"] for m in tail] == [m["id"] for m in all_metrics[-2:]]


class TestPriceUpdate:
    def test_a_price_update_moves_the_valuation(self):
        portfolio = _portfolio().json()
        holding = _holding(portfolio["id"], quantity=10.0, cost=50.0, price=50.0).json()
        body = client.post(f"/holdings/{holding['id']}/update-price", params={"new_price": 75.0}, headers=H1).json()
        assert body == {"holding_id": holding["id"], "new_price": 75.0}
        metric = client.post(f"/portfolios/{portfolio['id']}/performance", headers=H1).json()
        assert metric["total_value"] == 750.0

    def test_an_unknown_holding_is_a_404(self):
        assert client.post("/holdings/ghost/update-price", params={"new_price": 1.0}, headers=H1).status_code == 404

    def test_another_user_cannot_move_the_price(self):
        portfolio = _portfolio().json()
        holding = _holding(portfolio["id"], quantity=10.0, cost=50.0, price=50.0).json()
        response = client.post(f"/holdings/{holding['id']}/update-price", params={"new_price": 1.0}, headers=H2)
        assert response.status_code == 404
        metric = client.post(f"/portfolios/{portfolio['id']}/performance", headers=H1).json()
        assert metric["total_value"] == 500.0
