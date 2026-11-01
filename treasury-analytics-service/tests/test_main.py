"""Book-scoping and persistence tests for treasury-analytics-service (fake Neo4j harness).

Covers: KPI computation semantics, snapshot persistence with
latest-wins overwrite, /kpi read-back, ownership and Book isolation.
"""

import importlib.util
import os
from decimal import Decimal

import main
import pytest
from fastapi.testclient import TestClient
from treasury_analytics_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ta_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


def _req(company="co-ta", cash=500000, inflow=200000, outflow=150000, std=100000, inv=100000, fx=50000, **kw):
    p = {
        "company_id": company,
        "total_cash": cash,
        "monthly_inflow": inflow,
        "monthly_outflow": outflow,
        "short_term_debt": std,
        "total_debt": std * 3,
        "investments": inv,
        "fx_exposure": fx,
    }
    p.update(kw)
    return p


def test_analyze_kpi_semantics():
    resp = client.post("/analyze", json=_req(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    kpis = {k["name"]: k for k in data["kpis"]}
    assert len(kpis) == 6
    assert Decimal(kpis["Net Cash Flow"]["value"]) == Decimal(50000)
    assert kpis["Net Cash Flow"]["status"] == "good"
    # cash_adequacy = 500000/150000*30 = 100 days -> good (>90)
    assert Decimal(kpis["Cash Runway"]["value"]) == Decimal(100)
    assert kpis["Cash Runway"]["status"] == "good"
    # debt_service = 100000/200000*100 = 50 -> critical (not <50)
    assert Decimal(kpis["Debt Service Ratio"]["value"]) == Decimal(50)
    assert kpis["Debt Service Ratio"]["status"] == "critical"
    # yield = (100000*0.05)/500000*100 = 1 -> warning
    assert Decimal(kpis["Investment Yield"]["value"]) == Decimal(1)
    assert kpis["Investment Yield"]["status"] == "warning"
    # fx = min(100, 50000/500000*100) = 10 -> good
    assert Decimal(kpis["FX Risk Score"]["value"]) == Decimal(10)
    assert kpis["FX Risk Score"]["status"] == "good"
    assert Decimal(data["cash_adequacy_days"]) == Decimal(100)
    assert Decimal(data["debt_service_ratio"]) == Decimal(50)
    assert Decimal(data["investment_yield"]) == Decimal(1)
    assert Decimal(data["fx_risk_score"]) == Decimal(10)


def test_kpi_persistence_and_latest_wins():
    assert client.get("/kpi/co-ta", headers=H1).json() == {"company_id": "co-ta", "message": "Run /analyze first"}

    client.post("/analyze", json=_req(cash=500000), headers=H1)
    stored = client.get("/kpi/co-ta", headers=H1).json()
    assert Decimal(stored["cash_adequacy_days"]) == Decimal(100)
    assert len(stored["kpis"]) == 6

    # latest-wins: re-analyze overwrites the previous snapshot
    client.post("/analyze", json=_req(cash=100000), headers=H1)
    stored = client.get("/kpi/co-ta", headers=H1).json()
    assert Decimal(stored["cash_adequacy_days"]) == Decimal(20)
    # only one snapshot per company per caller
    snap_nodes = [n for n in _fake_session.nodes if n.get("label") == "TreasuryAnalyticsSnapshot"]
    assert len(snap_nodes) == 1

    # other user sees nothing
    assert client.get("/kpi/co-ta", headers=H2).json()["message"] == "Run /analyze first"


def test_book_a_b_isolation():
    hb = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    client.post("/analyze", json=_req(cash=500000), headers=H1)
    client.post("/analyze", json=_req(cash=900000), headers=hb)
    assert Decimal(client.get("/kpi/co-ta", headers=H1).json()["cash_adequacy_days"]) == Decimal(100)
    assert Decimal(client.get("/kpi/co-ta", headers=hb).json()["cash_adequacy_days"]) == Decimal(180)
    # personal view sees the Book-A snapshot when no Book header is sent
    # (fake default: no header -> Book filter passes all, both Books have one;
    #  per-Book views each return exactly their own)
    snap_nodes = [n for n in _fake_session.nodes if n.get("label") == "TreasuryAnalyticsSnapshot"]
    assert len(snap_nodes) == 2
    books = {n["props"]["book_id"] for n in snap_nodes}
    assert books == {BOOK_A, BOOK_B}


def test_edge_cases_kept():
    # zero outflow -> 999 runway; zero inflow -> 0 debt service
    data = client.post("/analyze", json=_req(outflow=0, inflow=0), headers=H1).json()
    assert Decimal(data["cash_adequacy_days"]) == Decimal(999)
    assert Decimal(data["debt_service_ratio"]) == Decimal(0)
    kpis = {k["name"]: k for k in data["kpis"]}
    assert kpis["Cash Runway"]["status"] == "good"
    assert kpis["Debt Service Ratio"]["status"] == "good"


def test_x_user_id_required():
    assert client.post("/analyze", json=_req()).status_code in (401, 403, 422)
    assert client.get("/kpi/co-ta").status_code in (401, 403, 422)


def test_any_amount_survives_analysis_and_storage():
    """A 30-digit book keeps its digits through the KPI maths and the snapshot."""
    huge = "123456789012345678901234567890.55"
    resp = client.post(
        "/analyze",
        json={
            "company_id": "co-huge",
            "total_cash": huge,
            "monthly_inflow": "1",
            "monthly_outflow": "1",
            "short_term_debt": "0",
            "total_debt": "0",
            "investments": "1",
            "fx_exposure": "0",
        },
        headers=H1,
    )
    assert resp.status_code == 200
    body = resp.json()
    # runway = huge / 1 * 30, to ten places, with no float truncation
    assert Decimal(body["cash_adequacy_days"]) == Decimal("3703703670370370367037037036716.5000000000")

    stored = client.get("/kpi/co-huge", headers=H1).json()
    # the snapshot round trip is exact, because amounts are stored as strings
    assert Decimal(stored["cash_adequacy_days"]) == Decimal(body["cash_adequacy_days"])
    assert isinstance(stored["kpis"][0]["value"], str)
