"""Book-scoping and persistence tests for currency-service (fake Neo4j harness)."""

import importlib.util
import os
import sys

import main
import pytest
from currency_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("curr_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "curr-user-1", "curr-user-2"
BOOK_A, BOOK_B = "curr-book-a", "curr-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def test_default_seeding_and_scoping():
    # first access seeds the original defaults for this user+Book
    resp = client.get("/currencies", headers=H1)
    assert resp.status_code == 200, resp.text
    currencies = resp.json()
    assert len(currencies) >= 15
    assert {c["code"] for c in currencies} >= {"USD", "EUR", "GBP", "JPY", "ZWL"} - {"ZWL"}

    # rates seeded too
    rates = client.get("/rates", headers=H1).json()
    assert any(r["from_currency"] == "USD" and r["to_currency"] == "EUR" for r in rates)

    # another user sees an independent set (seeded on their own access)
    other = client.get("/currencies", headers=H2).json()
    assert len(other) == len(currencies)

    # personal (no Book) scope for U1 also seeds independently
    personal = client.get("/currencies", headers=H1_PERSONAL).json()
    assert len(personal) >= 15

    # stats reflect the caller's own records
    stats = client.get("/stats", headers=H1).json()
    assert stats["total_currencies"] >= 15
    assert stats["total_rates"] >= 15


def test_currency_crud_and_isolation():
    # create
    resp = client.post(
        "/currencies",
        json={"code": "ZWL", "name": "Zimbabwe Gold", "symbol": "ZiG", "decimal_places": 2},
        headers=H1,
    )
    assert resp.status_code == 201, resp.text
    assert resp.json()["code"] == "ZWL"

    # duplicate: 400
    assert (
        client.post(
            "/currencies",
            json={"code": "zwl", "name": "dup", "symbol": "x", "decimal_places": 2},
            headers=H1,
        ).status_code
        == 400
    )

    # other user / other book cannot see it
    assert client.get("/currencies/ZWL", headers=H2).status_code == 404
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/currencies/ZWL", headers=other_book).status_code == 404

    # update persists for the caller only
    resp = client.put(
        "/currencies/ZWL",
        json={"code": "ZWL", "name": "Zimbabwe Gold RTGS", "symbol": "ZiG", "decimal_places": 2},
        headers=H1,
    )
    assert resp.status_code == 200
    assert client.get("/currencies/ZWL", headers=H1).json()["name"] == "Zimbabwe Gold RTGS"
    # cross-scope update: 404
    assert (
        client.put(
            "/currencies/ZWL", json={"code": "ZWL", "name": "hijack", "symbol": "x", "decimal_places": 2}, headers=H2
        ).status_code
        == 404
    )

    # soft delete keeps it listed but inactive
    assert client.delete("/currencies/ZWL", headers=H1).status_code == 204
    got = client.get("/currencies/ZWL", headers=H1).json()
    assert got["is_active"] is False
    assert client.get("/currencies", params={"active_only": True}, headers=H1).json() == [] or all(
        c["code"] != "ZWL" for c in client.get("/currencies", params={"active_only": True}, headers=H1).json()
    )


def test_rates_crud_history_and_isolation():
    # create a custom rate
    resp = client.post(
        "/rates",
        json={"from_currency": "usd", "to_currency": "eur", "rate": 0.95, "source": "manual"},
        headers=H1,
    )
    assert resp.status_code == 201, resp.text
    assert resp.json()["from_currency"] == "USD"

    # pair lookup returns the MOST RECENT rate
    got = client.get("/rates/USD/EUR", headers=H1).json()
    assert got["rate"] == 0.95

    # update creates a new entry (original semantics)
    upd = client.put("/rates/usd/eur", json={"rate": 0.91, "source": "api"}, headers=H1)
    assert upd.status_code == 200
    assert client.get("/rates/USD/EUR", headers=H1).json()["rate"] == 0.91

    # history keeps both
    hist = client.get("/rates/history/USD/EUR", headers=H1).json()
    assert hist["count"] >= 2

    # isolation: other user's pair lookup only sees their defaults
    other = client.get("/rates/USD/EUR", headers=H2).json()
    assert other["rate"] == 0.92  # their seeded default, not 0.91

    # unknown pair: 404
    assert client.get("/rates/USD/XYZ", headers=H1).status_code == 404

    # filters
    only_eur = client.get("/rates", params={"to_currency": "eur"}, headers=H1).json()
    assert all(r["to_currency"] == "EUR" for r in only_eur)


def test_conversion_paths():
    # direct
    resp = client.post("/convert", json={"from_currency": "USD", "to_currency": "EUR", "amount": 100}, headers=H1)
    assert resp.status_code == 200
    data = resp.json()
    assert data["rate_used"] == 0.92
    assert data["converted_amount"] == 92.0

    # inverse rate
    resp = client.post("/convert", json={"from_currency": "EUR", "to_currency": "USD", "amount": 92}, headers=H1)
    assert resp.json()["converted_amount"] == 100.0

    # cross via USD (JPY amounts use 0 decimal places)
    resp = client.post("/convert", json={"from_currency": "EUR", "to_currency": "JPY", "amount": 100}, headers=H1)
    assert resp.status_code == 200
    assert resp.json()["converted_amount"] == round(100 * 149.50 / 0.92)

    # unknown pair: 400
    assert (
        client.post(
            "/convert", json={"from_currency": "USD", "to_currency": "XYZ", "amount": 1}, headers=H1
        ).status_code
        == 400
    )

    # custom rate affects conversion for its owner only
    client.put("/rates/usd/eur", json={"rate": 0.91}, headers=H1)
    assert (
        client.post("/convert", json={"from_currency": "USD", "to_currency": "EUR", "amount": 100}, headers=H1).json()[
            "converted_amount"
        ]
        == 91.0
    )
    assert (
        client.post("/convert", json={"from_currency": "USD", "to_currency": "EUR", "amount": 100}, headers=H2).json()[
            "converted_amount"
        ]
        == 92.0
    )

    # batch: mixed success/failure accounting
    batch = client.post(
        "/convert/batch",
        json=[
            {"from_currency": "USD", "to_currency": "EUR", "amount": 10},
            {"from_currency": "USD", "to_currency": "XYZ", "amount": 10},
        ],
        headers=H1,
    ).json()
    assert batch["successful"] == 1
    assert batch["failed"] == 1

    # triangulation compares direct vs cross
    tri = client.post(
        "/triangulate", params={"from_currency": "EUR", "to_currency": "JPY", "amount": 100}, headers=H1
    ).json()
    assert tri["cross_conversion"] is not None
    tri_usd = client.post(
        "/triangulate", params={"from_currency": "USD", "to_currency": "EUR", "amount": 100}, headers=H1
    ).json()
    assert tri_usd["cross_conversion"] is None


def test_format_and_validation():
    # format uses the caller's currency metadata
    resp = client.get("/format/JPY/1234.5", headers=H1)
    assert resp.status_code == 200
    assert resp.json()["formatted"] == "¥1,234"  # format() uses round-half-even, original behavior

    resp = client.get("/format/EUR/1234.5678", headers=H1)
    assert resp.json()["formatted"] == "€1,234.57"

    # unknown currency: 404
    assert client.get("/format/XYZ/1", headers=H1).status_code == 404

    # validate-transaction converts each line into the base currency
    resp = client.post(
        "/validate-transaction",
        json={
            "base_currency": "USD",
            "lines": [
                {"currency": "USD", "amount": 50, "account": "1000", "description": "cash"},
                {"currency": "EUR", "amount": 92, "account": "1001", "description": "eur sale"},
            ],
        },
        headers=H1,
    )
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["valid"] is True
    assert data["line_count"] == 2
    assert data["total_in_base"] == 150.0
    assert data["lines"][1]["converted_to"] == "USD"
    assert data["lines"][1]["converted_amount"] == 100.0

    # unsupported base currency: 400
    assert (
        client.post("/validate-transaction", json={"base_currency": "XYZ", "lines": []}, headers=H1).status_code == 400
    )

    # missing line rate: 400 (original raised an unhandled 500)
    assert (
        client.post(
            "/validate-transaction",
            json={"base_currency": "USD", "lines": [{"currency": "XYZ", "amount": 1}]},
            headers=H1,
        ).status_code
        == 400
    )


def test_health_and_alert_stub():
    assert client.get("/").json()["status"] == "healthy"
    resp = client.post(
        "/alerts/rate",
        params={"from_currency": "USD", "to_currency": "EUR", "target_rate": 0.95, "direction": "above"},
    )
    assert resp.status_code == 200
    assert resp.json()["alert_id"] == "rate_alert_USD_EUR"
