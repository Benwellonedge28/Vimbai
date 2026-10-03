"""Book-scoping and persistence tests for treasury-reporting-service (fake Neo4j harness)."""

import importlib.util
import os

import main  # noqa: F401 (bootstraps the treasury_reporting_service package alias)
import pytest
from fastapi.testclient import TestClient
from treasury_reporting_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("tr_root_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "tr-user-1", "tr-user-2"
BOOK_A, BOOK_B = "tr-book-a", "tr-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _cash_report(period="2026-09", headers=H1):
    return client.post(
        "/reports/cash-position",
        params={"period": period, "generated_by": "CFO"},
        json=[
            {
                "account_id": "acc-1",
                "account_name": "ZB Bank",
                "currency": "USD",
                "balance": 5000.0,
                "balance_usd": 5000.0,
            },
            {
                "account_id": "acc-2",
                "account_name": "ZB ZWL",
                "currency": "ZWL",
                "balance": 900000.0,
                "balance_usd": 100.0,
            },
        ],
        headers=headers,
    )


def test_health():
    assert client.get("/health").json()["service"] == "treasury-reporting-service"


def test_cash_position_report_persists_and_scopes():
    r = _cash_report()
    assert r.status_code == 200
    rep = r.json()
    assert rep["report_type"] == "cash_position"
    assert rep["summary"]["total_cash_usd"] == 5100.0
    assert rep["summary"]["total_accounts"] == 2
    assert rep["summary"]["by_currency"] == {"USD": 5000.0, "ZWL": 900000.0}
    assert len(rep["data"]["entries"]) == 2

    # read back by id — persisted
    got = client.get(f"/reports/{rep['id']}", headers=H1).json()
    assert got["summary"]["total_cash_usd"] == 5100.0

    # foreign caller: 404 by id, invisible in listing
    assert client.get(f"/reports/{rep['id']}", headers=H2).status_code == 404
    assert client.get("/reports", headers=H2).json() == []
    # Book-gated
    other_book = dict(H1)
    other_book["X-Book-ID"] = BOOK_B
    assert client.get("/reports", headers=other_book).json() == []


def test_fx_exposure_and_debt_reports():
    r = client.post(
        "/reports/fx-exposure",
        params={"period": "2026-Q3"},
        json=[
            {
                "currency_pair": "USD/ZWL",
                "exposure_amount": 100000.0,
                "exposure_usd": 2500.0,
                "hedge_ratio": 0.5,
                "unhedged_amount": 1250.0,
            },
            {
                "currency_pair": "USD/ZAR",
                "exposure_amount": 50000.0,
                "exposure_usd": 2700.0,
                "hedge_ratio": 0.3,
                "unhedged_amount": 1890.0,
            },
        ],
        headers=H1,
    )
    body = r.json()
    assert body["summary"]["total_exposure_usd"] == 5200.0
    assert body["summary"]["total_unhedged_usd"] == 3140.0
    assert body["summary"]["average_hedge_ratio"] == 0.4
    assert body["summary"]["currency_pairs"] == 2

    d = client.post(
        "/reports/debt-portfolio",
        params={"period": "2026-Q3", "total_debt": 100000.0, "total_debt_usd": 100000.0, "weighted_avg_rate": 7.5},
        json=[{"id": "loan-1", "principal": 100000.0, "rate": 7.5}],
        headers=H1,
    ).json()
    assert d["summary"]["instrument_count"] == 1
    assert d["data"]["instruments"][0]["principal"] == 100000.0

    # list with type filter + tail limit
    assert len(client.get("/reports", headers=H1).json()) == 2
    assert len(client.get("/reports", params={"report_type": "fx_exposure"}, headers=H1).json()) == 1
    last2 = client.get("/reports", params={"limit": 2}, headers=H1).json()
    assert [r_["report_type"] for r_ in last2] == ["fx_exposure", "debt_portfolio"]
