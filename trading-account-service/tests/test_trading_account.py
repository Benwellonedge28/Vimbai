"""Book-scoping and arithmetic tests for trading-account-service."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from trading_account_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("trading_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "ta-user-1", "ta-user-2"
BOOK_A, BOOK_B = "ta-book-a", "ta-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

PAYLOAD = {
    "company_id": "acme",
    "period": "FY2026",
    "opening_stock": 20000.0,
    "purchases": 60000.0,
    "carriage_inward": 2000.0,
    "closing_stock": 25000.0,
    "sales": 120000.0,
    "sales_returns": 5000.0,
    "direct_wages": 8000.0,
    "factory_overhead": 5000.0,
}


def _generate(payload=None, headers=H1):
    return client.post("/generate", json={**PAYLOAD, **(payload or {})}, headers=headers)


class TestArithmetic:
    def test_the_trading_account_figures(self):
        body = _generate().json()
        # cost of goods sold = 20000 + 60000 + 2000 + 8000 + 5000 - 25000
        assert body["net_sales"] == 115000.0
        assert body["cost_of_goods_sold"] == 70000.0
        assert body["gross_profit"] == 45000.0
        assert body["gross_profit_pct"] == 39.13
        assert body["details"]["opening_stock"] == 20000.0

    def test_sales_returns_reduce_net_sales(self):
        body = _generate({"sales_returns": 15000.0}).json()
        assert body["net_sales"] == 105000.0
        assert body["gross_profit"] == 35000.0

    def test_zero_net_sales_gives_a_zero_margin_not_an_error(self):
        body = _generate({"sales": 0.0, "sales_returns": 0.0}).json()
        assert body["net_sales"] == 0.0
        assert body["gross_profit_pct"] == 0.0

    def test_a_loss_reports_a_negative_margin(self):
        body = _generate({"sales": 50000.0, "sales_returns": 0.0}).json()
        assert body["gross_profit"] < 0
        assert body["gross_profit_pct"] < 0


class TestPersistence:
    def test_a_generated_account_is_listed_back(self):
        created = _generate().json()
        listed = client.get("/accounts", headers=H1).json()
        assert [a["id"] for a in listed] == [created["id"]]
        assert listed[0]["gross_profit"] == 45000.0
        assert listed[0]["book_id"] == BOOK_A

    def test_the_period_filter_reads_the_flag(self):
        _generate({"period": "FY2026"})
        _generate({"period": "FY2025"})
        fy25 = client.get("/accounts", params={"period": "FY2025"}, headers=H1).json()
        assert len(fy25) == 1
        assert fy25[0]["period"] == "FY2025"

    def test_it_can_be_read_by_id(self):
        created = _generate().json()
        fetched = client.get(f"/accounts/{created['id']}", headers=H1)
        assert fetched.status_code == 200
        assert fetched.json()["period"] == "FY2026"

    def test_an_unknown_account_is_a_404(self):
        assert client.get("/accounts/ghost", headers=H1).status_code == 404

    def test_another_book_cannot_see_or_read_it(self):
        created = _generate().json()
        assert client.get("/accounts", headers=H1_BOOK_B).json() == []
        assert client.get(f"/accounts/{created['id']}", headers=H1_BOOK_B).status_code == 404

    def test_another_user_cannot_read_it(self):
        created = _generate().json()
        assert client.get("/accounts", headers=H2).json() == []
        assert client.get(f"/accounts/{created['id']}", headers=H2).status_code == 404
