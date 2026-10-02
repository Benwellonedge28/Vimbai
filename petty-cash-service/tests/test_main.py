"""Book-scoping and persistence tests for petty-cash-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from petty_cash_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("petty_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "petty-user-1", "petty-user-2"
BOOK_A, BOOK_B = "petty-book-a", "petty-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _fund_payload(code="PC-001", **kw):
    payload = {
        "id": "placeholder-id",
        "fund_code": code,
        "fund_name": "Head Office Petty Cash",
        "custodian_id": "cust-1",
        "custodian_name": "Tendai Moyo",
        "location": "Harare HQ",
        "maximum_balance": "500.00",
        "minimum_balance": "50.00",
        "replenishment_threshold": "100.00",
        "replenishment_amount": "400.00",
        "status": "active",
        "account_code": "1010-PETTY",
    }
    payload.update(kw)
    return payload


def _tx_payload(fund_id, tx_type="initial_fund", amount="300.00", **kw):
    payload = {
        "id": "placeholder-id",
        "fund_id": fund_id,
        "transaction_type": tx_type,
        "amount": amount,
        "date": "2026-10-01T08:00:00+00:00",
        "description": "Opening float",
        "category": "miscellaneous",
        "reference_number": "REF-1",
        "voucher_number": "V-000",
        "entered_by": "petty-user-1",
    }
    payload.update(kw)
    return payload


def _create_fund(headers=H1, **kw):
    resp = client.post("/funds", json=_fund_payload(**kw), headers=headers)
    assert resp.status_code == 200, resp.text
    return resp.json()


def _seed_fund_with_float(headers=H1, amount="300.00"):
    fund = _create_fund(headers)
    resp = client.post("/transactions", json=_tx_payload(fund["id"], amount=amount), headers=headers)
    assert resp.status_code == 200, resp.text
    return fund


# --- fund CRUD + persistence ---


def test_fund_create_get_update_close_persists():
    fund = _create_fund()
    assert fund["id"]
    assert fund["book_id"] == BOOK_A
    assert fund["maximum_balance"] == "500.00"  # exact decimal round-trip

    got = client.get(f"/funds/{fund['id']}", headers=H1).json()
    assert got["fund_name"] == "Head Office Petty Cash"

    # update persists
    upd = client.put(f"/funds/{fund['id']}", json=_fund_payload(fund_name="Branch Petty Cash"), headers=H1).json()
    assert upd["fund_name"] == "Branch Petty Cash"
    assert client.get(f"/funds/{fund['id']}", headers=H1).json()["fund_name"] == "Branch Petty Cash"

    # close persists
    resp = client.post(f"/funds/{fund['id']}/close", params={"closed_by": "manager"}, headers=H1)
    assert resp.status_code == 200
    assert client.get(f"/funds/{fund['id']}", headers=H1).json()["status"] == "closed"


def test_fund_user_isolation_and_book_gating():
    fund = _create_fund()

    # other user (even same book) sees nothing: caller-owned
    assert client.get("/funds", headers=H2).json() == []
    assert client.get(f"/funds/{fund['id']}", headers=H2).status_code == 404

    # same user, different book: invisible (Book gate)
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get(f"/funds/{fund['id']}", headers=other_book).status_code == 404
    assert client.get("/funds", headers=other_book).json() == []

    # personal (no book) can still reach the record it owns
    assert client.get(f"/funds/{fund['id']}", headers=H1_PERSONAL).status_code == 200

    # cross-scope update is refused
    resp = client.put(f"/funds/{fund['id']}", json=_fund_payload(), headers=H2)
    assert resp.status_code == 404


def test_fund_list_filters():
    _create_fund(code="PC-A", custodian_id="c1")
    _create_fund(code="PC-B", custodian_id="c2")
    assert len(client.get("/funds", headers=H1).json()) == 2
    assert len(client.get("/funds", params={"custodian_id": "c2"}, headers=H1).json()) == 1


# --- transactions + balance ---


def test_transaction_balance_and_minimum_guard():
    fund = _seed_fund_with_float()  # 300 in

    resp = client.post("/transactions", json=_tx_payload(fund["id"], tx_type="payment", amount="100.00"), headers=H1)
    assert resp.status_code == 200, resp.text

    # payment below the 50.00 minimum is rejected
    resp = client.post("/transactions", json=_tx_payload(fund["id"], tx_type="payment", amount="180.00"), headers=H1)
    assert resp.status_code == 422
    assert "Insufficient balance" in resp.text

    bal = client.get(f"/funds/{fund['id']}/balance", headers=H1).json()
    assert bal["total_receipts"] == "300.00"
    assert bal["total_payments"] == "100.00"
    assert bal["current_balance"] == "200.00"

    # transaction against another user's fund: 404
    resp = client.post("/transactions", json=_tx_payload(fund["id"]), headers=H2)
    assert resp.status_code == 404

    listed = client.get("/transactions", params={"fund_id": fund["id"]}, headers=H1).json()
    assert len(listed) == 2


def test_summary_replenishment_flag():
    fund = _seed_fund_with_float(amount="300.00")
    # spend down to 80 (below the 100.00 threshold)
    resp = client.post(
        "/transactions",
        json=_tx_payload(fund["id"], tx_type="payment", amount="220.00", description="stationery run"),
        headers=H1,
    )
    assert resp.status_code == 200

    summary = client.get(f"/funds/{fund['id']}/summary", headers=H1).json()
    assert summary["closing_balance"] == "80.00"
    assert summary["replenishment_needed"] is True
    assert summary["total_receipts"] == "300.00"
    assert summary["total_payments"] == "220.00"


# --- vouchers ---


def test_voucher_creates_payment_tx_and_approve_persists():
    fund = _seed_fund_with_float()

    voucher_payload = {
        "id": "placeholder-id",
        "fund_id": fund["id"],
        "voucher_number": "V-100",
        "date": "2026-10-02T09:00:00+00:00",
        "payee": "Office Mart",
        "amount": "60.00",
        "description": "Printer paper",
        "category": "office_supplies",
        "entered_by": "petty-user-1",
    }
    resp = client.post("/vouchers", json=voucher_payload, headers=H1)
    assert resp.status_code == 200, resp.text
    voucher = resp.json()["voucher"]

    # the corresponding payment transaction exists
    tx_id = resp.json()["transaction_id"]
    tx = client.get(f"/transactions/{tx_id}", headers=H1).json()
    assert tx["transaction_type"] == "payment"
    assert tx["amount"] == "60.00"
    assert tx["recipient_name"] == "Office Mart"

    # approve persists
    approved = client.put(f"/vouchers/{voucher['id']}/approve", params={"approved_by": "manager"}, headers=H1).json()
    assert approved["status"] == "approved"
    assert approved["approved_by"] == "manager"
    listed = client.get("/vouchers", params={"status": "approved"}, headers=H1).json()
    assert len(listed) == 1

    # cross-user approve: 404
    assert client.put(f"/vouchers/{voucher['id']}/approve", params={"approved_by": "x"}, headers=H2).status_code == 404


# --- replenishments ---


def test_replenishment_flow_persists():
    fund = _seed_fund_with_float()
    repl_payload = {
        "id": "placeholder-id",
        "fund_id": fund["id"],
        "amount": "400.00",
        "requested_by": "petty-user-1",
        "request_date": "2026-10-02T10:00:00+00:00",
        "transactions_included": [],
    }
    resp = client.post("/replenishments", json=repl_payload, headers=H1)
    assert resp.status_code == 200, resp.text
    repl = resp.json()
    assert repl["status"] == "pending"

    # fund went REPLENISHING
    assert client.get(f"/funds/{fund['id']}", headers=H1).json()["status"] == "replenishing"

    # approve: creates REPLENISHMENT tx + fund back to ACTIVE
    resp = client.put(f"/replenishments/{repl['id']}/approve", params={"approved_by": "manager"}, headers=H1)
    assert resp.status_code == 200, resp.text
    approved = resp.json()["replenishment"]
    assert approved["status"] == "approved"
    assert approved["approved_by"] == "manager"

    tx_id = resp.json()["transaction_id"]
    tx = client.get(f"/transactions/{tx_id}", headers=H1).json()
    assert tx["transaction_type"] == "replenishment"

    assert client.get(f"/funds/{fund['id']}", headers=H1).json()["status"] == "active"

    # balance now includes the replenishment
    bal = client.get(f"/funds/{fund['id']}/balance", headers=H1).json()
    assert bal["current_balance"] == "700.00"

    # cross-user approve: 404
    assert (
        client.put(f"/replenishments/{repl['id']}/approve", params={"approved_by": "x"}, headers=H2).status_code == 404
    )


def test_replenishment_unknown_fund_ignored_gracefully():
    """Original behavior: replenishment for a missing fund is still stored."""
    payload = {
        "id": "placeholder-id",
        "fund_id": "no-such-fund",
        "amount": "50.00",
        "requested_by": U1,
        "request_date": "2026-10-02T10:00:00+00:00",
        "transactions_included": [],
    }
    resp = client.post("/replenishments", json=payload, headers=H1)
    assert resp.status_code == 200, resp.text
    assert resp.json()["fund_id"] == "no-such-fund"


# --- reports ---


def test_reports_scoped_to_caller():
    f1 = _seed_fund_with_float()  # user1: 300
    # user2 gets their own fund with a different position
    fund2 = _create_fund(headers=H2, code="PC-U2", custodian_name="B Anjuru")
    resp = client.post("/transactions", json=_tx_payload(fund2["id"], amount="1000.00"), headers=H2)
    assert resp.status_code == 200

    pos1 = client.get("/reports/cash-position", headers=H1).json()
    assert pos1["total_funds"] == 1
    assert pos1["total_cash"] == "300.00"

    pos2 = client.get("/reports/cash-position", headers=H2).json()
    assert pos2["total_funds"] == 1
    assert pos2["total_cash"] == "1000.00"

    # category summary only counts the caller's payments
    client.post(
        "/transactions",
        json=_tx_payload(f1["id"], tx_type="payment", amount="40.00", category="meals"),
        headers=H1,
    )
    cat1 = client.get("/reports/category-summary", headers=H1).json()
    assert cat1["total_transactions"] == 1
    assert cat1["total_amount"] == 40.0

    # fund report: 404 for another user's fund
    assert client.get(f"/reports/fund-report/{fund2['id']}", headers=H1).status_code == 404


def test_health():
    resp = client.get("/")
    assert resp.status_code == 200
    assert resp.json()["status"] == "healthy"
