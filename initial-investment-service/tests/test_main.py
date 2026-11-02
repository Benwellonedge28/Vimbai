"""Book-scoping, persistence and arithmetic tests for initial-investment-service.

Covers the itemised investment build-up, the stateless simple and capital-allowance
helpers, listing and lookup (including the original 200-with-error contract), and
cross-user / cross-Book isolation.

Everything investment used to sit in a process-global list shared by every tenant;
it is now grep-able only through the caller's own Book.
"""

import importlib.util
import os

import main  # noqa: F401  (bare import first: main self-bootstraps the package)
import pytest
from fastapi.testclient import TestClient
from initial_investment_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("inv_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "inv-user-1", "inv-user-2"
BOOK_A, BOOK_B = "inv-book-a", "inv-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


BASE = {
    "project_name": "New line",
    "asset_cost": 100000.0,
    "installation_cost": 5000.0,
    "working_capital": 20000.0,
    "other_costs": 0.0,
    "scrap_value": 10000.0,
}


def _create(headers=H1, **overrides):
    params = dict(BASE)
    params.update(overrides)
    return client.post("/create", params=params, headers=headers).json()


def test_itemised_investment_totals():
    body = _create()
    assert body["total_investment"] == 125000.0
    assert body["net_investment"] == 115000.0
    assert [i["category"] for i in body["items"]] == ["asset", "installation", "working_capital"]


def test_zero_value_lines_are_not_itemised():
    body = _create(installation_cost=0.0, working_capital=0.0)
    assert [i["item"] for i in body["items"]] == ["Asset Cost"]
    assert body["total_investment"] == 100000.0


def test_simple_outlay_nets_sale_proceeds():
    body = client.post(
        "/simple",
        params={"asset_cost": 100000.0, "scrap_value": 10000.0, "selling_of_old_asset": 30000.0, "tax_on_sale": 4000.0},
        headers=H1,
    ).json()
    assert body["net_proceeds_from_sale"] == 26000.0
    assert body["initial_cash_outlay"] == 74000.0


def test_capital_allowance_shield_uses_a_percentage_rate():
    body = client.post(
        "/with-capital-allowance",
        params={"asset_cost": 100000.0, "scrap_value": 0.0, "capital_allowance": 25000.0, "tax_rate": 30.0},
        headers=H1,
    ).json()
    assert body["tax_saving"] == 7500.0
    assert body["adjusted_investment_cost"] == 92500.0


def test_investments_persist_and_are_retrievable():
    created = _create()
    listed = client.get("/list", headers=H1).json()["investments"]
    assert [i["id"] for i in listed] == [created["id"]]

    fetched = client.get(f"/get/{created['id']}", headers=H1).json()
    assert fetched["net_investment"] == 115000.0
    assert fetched["items"][0]["amount"] == 100000.0


def test_unknown_investment_keeps_the_original_error_body():
    assert client.get("/get/nope", headers=H1).json() == {"error": "Investment not found"}


def test_investments_are_caller_and_book_scoped():
    created = _create()
    assert client.get("/list", headers=H2).json()["investments"] == []
    assert client.get("/list", headers=HB).json()["investments"] == []
    assert client.get(f"/get/{created['id']}", headers=H2).json() == {"error": "Investment not found"}


def test_caller_header_is_required_for_reads():
    assert client.get("/list").status_code == 422


def test_health_reports_the_service():
    body = client.get("/health").json()
    assert body["service"] == "initial-investment-service"
