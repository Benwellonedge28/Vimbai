"""Book-scoping and persistence tests for company-accounting-service (fake Neo4j harness).

Covers: record persistence across requests, ownership + Book isolation,
cross-scope 404s, computed semantics (percentages, dividends, reserves,
retained earnings), and report assembly over the caller's records only.
"""

import importlib.util
import os
from decimal import Decimal

import main  # noqa: F401  (isort: keep bare main import first)
import pytest
from company_accounting_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("cac_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


def dec(v):
    """Coerce a JSON number (FastAPI renders Decimal as float) back to Decimal."""
    return Decimal(str(v))


U1, U2 = "cac-user-1", "cac-user-2"
BOOK_A, BOOK_B = "cac-book-a", "cac-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _company(name="Testco", code="TCO", headers=H1, **kw):
    payload = {
        "id": "",
        "company_code": code,
        "company_name": name,
        "company_type": "limited_company",
        "incorporation_date": "2024-01-15T00:00:00Z",
        "financial_year_end": "December",
        "jurisdiction": "Zimbabwe",
    }
    payload.update(kw)
    r = client.post("/companies", json=payload, headers=headers)
    assert r.status_code == 200, r.text
    return r.json()


def _shareholder(company_id, name, shares, headers=H1, share_class="ordinary"):
    r = client.post(
        "/shareholders",
        json={
            "id": "",
            "company_id": company_id,
            "shareholder_name": name,
            "shareholder_type": "individual",
            "share_class": share_class,
            "shares_held": shares,
            "percentage_holding": 0,
            "registration_date": "2024-02-01T00:00:00Z",
        },
        headers=headers,
    )
    assert r.status_code == 200, r.text
    return r.json()


def test_company_persists_and_is_owned():
    c = _company()
    assert c["id"] and c["status"] == "active"

    # persists across a fresh request
    got = client.get(f"/companies/{c['id']}", headers=H1).json()
    assert got["company_name"] == "Testco"

    # update persists
    r = client.put(
        f"/companies/{c['id']}",
        json={**c, "company_name": "Renamed", "id": c["id"]},
        headers=H1,
    )
    assert r.status_code == 200
    assert client.get(f"/companies/{c['id']}", headers=H1).json()["company_name"] == "Renamed"


def test_cross_user_access_404():
    c = _company()
    assert client.get(f"/companies/{c['id']}", headers=H2).status_code == 404
    assert (
        client.put(
            f"/companies/{c['id']}",
            json={**c, "company_name": "Hijacked"},
            headers=H2,
        ).status_code
        == 404
    )
    assert client.get("/companies", headers=H2).json() == []


def test_book_isolation():
    ca = _company(name="BookA-Co", headers=H1)
    _company(name="BookB-Co", headers=HB)

    # same user, different Books: each sees only its own Book's records
    a = [c["company_name"] for c in client.get("/companies", headers=H1).json()]
    b = [c["company_name"] for c in client.get("/companies", headers=HB).json()]
    assert a == ["BookA-Co"]
    assert b == ["BookB-Co"]
    assert client.get(f"/companies/{ca['id']}", headers=HB).status_code == 404


def test_personal_context_sees_own_records():
    c = _company(name="Solo-Co", headers={"X-User-Id": U1})
    names = [x["company_name"] for x in client.get("/companies", headers={"X-User-Id": U1}).json()]
    assert names == ["Solo-Co"]


def test_shareholder_percentage_recalc_and_isolation():
    c1 = _company(name="Pct-Co1")
    _company(name="Pct-Co2", headers=H2)

    s1 = _shareholder(c1["id"], "Alice", 30)
    s2 = _shareholder(c1["id"], "Bob", 70)

    holders = sorted(client.get("/shareholders", headers=H1).json(), key=lambda s: s["shareholder_name"])
    assert holders[0]["percentage_holding"] == pytest.approx(30.0)
    assert holders[1]["percentage_holding"] == pytest.approx(70.0)
    assert s1["percentage_holding"] == pytest.approx(100.0)  # single holder at creation

    # other user sees none of these shareholders
    assert client.get("/shareholders", headers=H2).json() == []


def test_share_capital_latest_wins():
    c = _company(name="Cap-Co")
    for issued in (100, 200):
        r = client.post(
            "/share-capital",
            json={
                "id": "",
                "company_id": c["id"],
                "share_class": "ordinary",
                "authorized_shares": 1000,
                "issued_shares": issued,
                "paid_up_value_per_share": "1.50",
                "total_paid_up_capital": "0",
                "as_of_date": "2024-03-01T00:00:00Z",
            },
            headers=H1,
        )
        assert r.status_code == 200, r.text
        assert dec(r.json()["total_paid_up_capital"]) == Decimal(str(issued)) * Decimal("1.50")

    latest = client.get("/share-capital", params={"company_id": c["id"]}, headers=H1).json()
    assert latest["issued_shares"] == 200
    assert dec(latest["total_paid_up_capital"]) == Decimal("300.00")

    # Book B cannot see it
    assert client.get("/share-capital", params={"company_id": c["id"]}, headers=HB).json() is None


def test_capital_transaction_isolation():
    ca = _company(name="Tx-CoA")
    cb = _company(name="Tx-CoB", headers=HB)

    for comp, headers in ((ca, H1), (cb, HB)):
        r = client.post(
            "/capital-transactions",
            json={
                "id": "",
                "company_id": comp["id"],
                "transaction_type": "share_issuance",
                "transaction_date": "2024-04-01T00:00:00Z",
                "number_of_shares": 100,
                "price_per_share": "2.00",
                "total_amount": "200.00",
                "reference_number": "REF-1",
            },
            headers=headers,
        )
        assert r.status_code == 200, r.text

    txs_a = client.get("/capital-transactions", params={"company_id": ca["id"]}, headers=H1).json()
    txs_b = client.get("/capital-transactions", params={"company_id": cb["id"]}, headers=HB).json()
    assert len(txs_a) == 1 and txs_a[0]["company_id"] == ca["id"]
    assert len(txs_b) == 1 and txs_b[0]["company_id"] == cb["id"]


def test_dividend_generates_payments_for_own_shareholders_only():
    c1 = _company(name="Div-CoA")
    c2 = _company(name="Div-CoB", headers=HB)
    _shareholder(c1["id"], "Alice", 100, headers=H1)
    _shareholder(c2["id"], "Mallory", 500, headers=HB)

    r = client.post(
        "/dividends",
        json={
            "id": "",
            "company_id": c1["id"],
            "dividend_type": "final",
            "declaration_date": "2024-05-01T00:00:00Z",
            "record_date": "2024-05-10T00:00:00Z",
            "per_share_amount": "0.10",
            "total_amount": "10.00",
            "net_payment": "0",
        },
        headers=H1,
    )
    assert r.status_code == 200, r.text
    div = r.json()
    assert dec(div["net_payment"]) == Decimal("10.00")

    payments = client.get(f"/dividends/{div['id']}/payments", headers=H1).json()
    assert len(payments) == 1
    assert payments[0]["shareholder_name"] == "Alice"
    assert dec(payments[0]["net_amount"]) == Decimal("9.00")  # 10% withholding

    # Book B cannot read the dividend or its payments
    assert client.get(f"/dividends/{div['id']}/payments", headers=HB).status_code == 404


def test_dividend_pay_flow_persists_and_is_gated():
    c = _company(name="Pay-Co")
    _shareholder(c["id"], "Alice", 100)
    div = client.post(
        "/dividends",
        json={
            "id": "",
            "company_id": c["id"],
            "dividend_type": "interim",
            "declaration_date": "2024-05-01T00:00:00Z",
            "record_date": "2024-05-10T00:00:00Z",
            "per_share_amount": "0.05",
            "total_amount": "5.00",
            "net_payment": "0",
        },
        headers=H1,
    ).json()

    # cross-scope pay 404s
    assert client.post(f"/dividends/{div['id']}/pay", params={"approved_by": "CEO"}, headers=HB).status_code == 404

    paid = client.post(f"/dividends/{div['id']}/pay", params={"approved_by": "CEO"}, headers=H1).json()
    assert paid["status"] == "paid"
    assert paid["approved_by"] == "CEO"

    payments = client.get(f"/dividends/{div['id']}/payments", headers=H1).json()
    assert all(p["status"] == "processed" for p in payments)
    assert all(p["payment_date"] is not None for p in payments)


def test_retained_earnings_computed_and_isolated():
    c = _company(name="RE-Co")
    r = client.post(
        "/retained-earnings",
        json={
            "id": "",
            "company_id": c["id"],
            "period_start": "2024-01-01T00:00:00Z",
            "period_end": "2024-12-31T00:00:00Z",
            "opening_balance": "1000.00",
            "net_profit_for_period": "500.00",
            "dividends_declared": "200.00",
            "closing_balance": "0",
        },
        headers=H1,
    )
    assert r.status_code == 200, r.text
    assert dec(r.json()["closing_balance"]) == Decimal("1300.00")

    assert client.get("/retained-earnings", params={"company_id": c["id"]}, headers=H2).json() == []


def test_reserves_computed_and_isolated():
    c = _company(name="Res-Co")
    r = client.post(
        "/reserves",
        json={
            "id": "",
            "company_id": c["id"],
            "reserve_name": "Statutory reserve",
            "reserve_type": "statutory",
            "opening_balance": "100.00",
            "transfers_in": "50.00",
            "transfers_out": "10.00",
            "closing_balance": "0",
            "as_of_date": "2024-06-30T00:00:00Z",
        },
        headers=H1,
    )
    assert r.status_code == 200, r.text
    assert dec(r.json()["closing_balance"]) == Decimal("140.00")

    assert client.get("/reserves", params={"company_id": c["id"]}, headers=H2).json() == []


def test_equity_statement_assembles_caller_records_only():
    c = _company(name="Eq-Co")
    _company(name="Eq-Other", headers=H2)
    _shareholder(c["id"], "Alice", 100)

    client.post(
        "/share-capital",
        json={
            "id": "",
            "company_id": c["id"],
            "share_class": "ordinary",
            "authorized_shares": 1000,
            "issued_shares": 100,
            "paid_up_value_per_share": "1.00",
            "total_paid_up_capital": "0",
            "as_of_date": "2024-03-01T00:00:00Z",
        },
        headers=H1,
    )
    client.post(
        "/capital-transactions",
        json={
            "id": "",
            "company_id": c["id"],
            "transaction_type": "share_issuance",
            "transaction_date": "2024-04-01T00:00:00Z",
            "number_of_shares": 100,
            "price_per_share": "1.00",
            "total_amount": "100.00",
            "reference_number": "EQ-1",
        },
        headers=H1,
    )

    r = client.get("/reports/equity-statement/" + c["id"], params={"as_of_date": "2024-12-31T00:00:00Z"}, headers=H1)
    assert r.status_code == 200, r.text
    report = r.json()
    assert dec(report["share_capital"]) == Decimal("100.00")
    assert len(report["movements"]) == 1
    assert dec(report["total_equity"]) == Decimal("100.00")

    # cross-scope: unknown company for the other caller
    assert (
        client.get(
            "/reports/equity-statement/" + c["id"], params={"as_of_date": "2024-12-31T00:00:00Z"}, headers=H2
        ).status_code
        == 404
    )


def test_shareholder_register_gated():
    c = _company(name="Reg-Co")
    _shareholder(c["id"], "Alice", 60)
    _shareholder(c["id"], "Bob", 40)

    r = client.get("/reports/shareholder-register/" + c["id"], headers=H1)
    assert r.status_code == 200
    body = r.json()
    assert body["total_shareholders"] == 2
    assert body["shareholders"][0]["name"] == "Alice"  # 60% first

    assert client.get("/reports/shareholder-register/" + c["id"], headers=H2).status_code == 404


def test_dividend_history_isolated():
    c = _company(name="Hist-Co")
    _company(name="Hist-Other", headers=H2)

    client.post(
        "/dividends",
        json={
            "id": "",
            "company_id": c["id"],
            "dividend_type": "special",
            "declaration_date": "2024-07-01T00:00:00Z",
            "record_date": "2024-07-10T00:00:00Z",
            "per_share_amount": "1.00",
            "total_amount": "100.00",
            "net_payment": "0",
        },
        headers=H1,
    )

    mine = client.get("/reports/dividend-history/" + c["id"], headers=H1).json()
    assert mine["total_dividends_declared"] == 1
    assert mine["total_amount"] == "100.00"

    theirs = client.get("/reports/dividend-history/" + c["id"], headers=H2).json()
    assert theirs["total_dividends_declared"] == 0
    assert theirs["total_amount"] == "0"
