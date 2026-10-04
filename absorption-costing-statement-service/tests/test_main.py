"""Book-scoping, persistence and behaviour tests for absorption-costing-statement-service (fake Neo4j harness).

Covers: trading account generation (COGS, gross profit, line items),
add-expenses mutation (net profit, completed status, upsert), production
cost statement math (materials used, prime cost, production cost, per
unit), caller-scoped listings/filters, error contracts, and cross-user /
cross-Book isolation.
"""

import importlib.util
import os
from datetime import datetime

import main  # noqa: F401  (isort: keep bare main import first)
import pytest
from absorption_costing_statement_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("acst_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "acst-user-1", "acst-user-2"
BOOK_A, BOOK_B = "acst-book-a", "acst-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

TRADE_PARAMS = {
    "company_id": "co-1",
    "period_start": "2026-09-01T00:00:00",
    "period_end": "2026-09-30T00:00:00",
    "opening_stock": 1000.0,
    "purchases": 5000.0,
    "carriage_inwards": 200.0,
    "closing_stock": 1500.0,
    "sales_revenue": 9000.0,
}


def _trading(params=None, headers=H1):
    p = dict(TRADE_PARAMS)
    if params:
        p.update(params)
    r = client.post("/trading-account/generate", params=p, headers=headers)
    assert r.status_code == 200, r.text
    return r.json()


def _production(product_id="PROD-1", period="2026-09", headers=H1, **extra):
    p = {
        "product_id": product_id,
        "period": period,
        "direct_materials_opening": 500.0,
        "direct_materials_purchases": 2000.0,
        "direct_materials_closing": 300.0,
        "direct_labor": 800.0,
        "direct_expenses": 100.0,
        "factory_overhead": 400.0,
        "work_in_progress_opening": 50.0,
        "work_in_progress_closing": 70.0,
        "units_produced": 100,
    }
    p.update(extra)
    r = client.post("/production-cost/generate", params=p, headers=headers)
    assert r.status_code == 200, r.text
    return r.json()


def test_trading_account_generation_and_persistence():
    s = _trading()

    assert s["cost_of_goods_sold"] == 4700.0
    assert s["gross_profit"] == 4300.0
    assert s["status"] == "draft"
    assert len(s["line_items"]) == 8
    assert s["line_items"][0]["description"] == "Sales Revenue"
    assert s["line_items"][-1]["amount"] == 4300.0

    listed = client.get("/trading-account", headers=H1).json()["statements"]
    assert len(listed) == 1
    assert listed[0]["id"] == s["id"]


def test_add_expenses_mutation_persists():
    s = _trading()
    r = client.post(
        "/trading-account/{}/add-expenses".format(s["id"]),
        params={"distribution_costs": 500.0, "administrative_expenses": 1000.0, "other_expenses": 300.0},
        headers=H1,
    )
    assert r.status_code == 200
    body = r.json()
    assert body["net_profit"] == 2500.0
    assert body["status"] == "completed"
    assert len(body["line_items"]) == 13

    # the mutation persisted (single record, not a duplicate)
    listed = client.get("/trading-account", headers=H1).json()["statements"]
    assert len(listed) == 1
    assert listed[0]["net_profit"] == 2500.0
    assert listed[0]["status"] == "completed"

    # fetch by id reflects the completed statement
    got = client.get("/trading-account/{}".format(s["id"]), headers=H1).json()
    assert got["net_profit"] == 2500.0

    # unknown statement keeps the original error contract
    assert client.post("/trading-account/NOPE/add-expenses", headers=H1).json() == {"error": "Statement not found"}


def test_production_cost_math_and_filters():
    s = _production()

    assert s["direct_materials_used"] == 2200.0
    assert s["prime_cost"] == 3100.0
    assert s["production_cost"] == 3480.0
    assert s["cost_per_unit"] == 34.8

    # zero units: per-unit stays 0
    s0 = _production(product_id="PROD-0", units_produced=0)
    assert s0["cost_per_unit"] == 0.0

    _production(product_id="PROD-1", period="2026-10")
    _production(product_id="PROD-2", period="2026-09")

    assert len(client.get("/production-cost", params={"product_id": "PROD-1"}, headers=H1).json()["statements"]) == 2
    # 2026-09 has PROD-1, PROD-0 (zero-units case) and PROD-2
    assert len(client.get("/production-cost", params={"period": "2026-09"}, headers=H1).json()["statements"]) == 3
    assert (
        len(
            client.get("/production-cost", params={"product_id": "PROD-1", "period": "2026-10"}, headers=H1).json()[
                "statements"
            ]
        )
        == 1
    )


def test_get_statement_and_error_contract():
    s = _trading()
    got = client.get("/trading-account/{}".format(s["id"]), headers=H1).json()
    assert got["company_id"] == "co-1"
    assert client.get("/trading-account/NOPE", headers=H1).json() == {"error": "Statement not found"}


def test_cross_user_and_book_isolation():
    s1 = _trading(params={"company_id": "co-A"}, headers=H1)
    _trading(params={"company_id": "co-B"}, headers=H2)
    _trading(params={"company_id": "co-C"}, headers=HB)

    # listings only see the caller's own statements
    listed = client.get("/trading-account", headers=H1).json()["statements"]
    assert len(listed) == 1
    assert listed[0]["company_id"] == "co-A"

    # get by id is caller-scoped
    assert client.get("/trading-account/{}".format(s1["id"]), headers=H2).json() == {"error": "Statement not found"}
    assert client.get("/trading-account/{}".format(s1["id"]), headers=HB).json() == {"error": "Statement not found"}

    # add-expenses cannot mutate another caller's statement
    r = client.post("/trading-account/{}/add-expenses".format(s1["id"]), params={"distribution_costs": 1.0}, headers=H2)
    assert r.json() == {"error": "Statement not found"}

    # production statements isolated too
    _production(product_id="PROD-ISO", headers=H1)
    _production(product_id="PROD-ISO", headers=H2)
    assert len(client.get("/production-cost", params={"product_id": "PROD-ISO"}, headers=H1).json()["statements"]) == 1
    assert len(client.get("/production-cost", params={"product_id": "PROD-ISO"}, headers=H2).json()["statements"]) == 1
