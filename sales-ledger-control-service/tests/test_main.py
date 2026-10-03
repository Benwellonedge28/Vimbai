"""Book-scoping and persistence tests for sales-ledger-control-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from sales_ledger_control_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("slc_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "slc-user-1", "slc-user-2"
BOOK_A, BOOK_B = "slc-book-a", "slc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}

D = "2026-01-01T10:00:00"


def _txn(client, headers, **kw):
    params = {"debtor_id": "cust-1", "debtor_name": "Customer One", "date": D, "amount": 100.0}
    params.update(kw)
    return client.post("/transactions", params=params, headers=headers)


def test_health_and_root():
    assert client.get("/health").json()["status"] == "healthy"
    assert "sales-ledger-control" in client.get("/").json()["service"]


def test_transaction_balances_sequence():
    r1 = _txn(client, H1, transaction_type="invoice", amount=100.0)
    assert r1.status_code == 200, r1.text
    assert r1.json()["balance"] == 100.0

    assert _txn(client, H1, transaction_type="payment", amount=40.0).json()["balance"] == 60.0
    assert _txn(client, H1, transaction_type="credit_note", amount=10.0).json()["balance"] == 50.0
    assert _txn(client, H1, transaction_type="refund", amount=5.0).json()["balance"] == 45.0
    assert _txn(client, H1, transaction_type="bad_debt", amount=5.0).json()["balance"] == 40.0

    # second debtor independent
    r6 = _txn(client, H1, transaction_type="invoice", debtor_id="cust-2", amount=200.0)
    assert r6.json()["balance"] == 200.0

    # per-debtor balance endpoint: count + last 10 txns
    resp = client.get("/debtors/cust-1/balance", headers=H1).json()
    assert resp["debtor_id"] == "cust-1"
    assert resp["balance"] == 40.0
    assert resp["transaction_count"] == 5
    assert len(resp["transactions"]) == 5

    # unknown debtor: zero balance, no txns
    assert client.get("/debtors/nobody/balance", headers=H1).json()["balance"] == 0


def test_debtors_list_and_isolation():
    _txn(client, H1, transaction_type="invoice", amount=100.0)
    _txn(client, H1, transaction_type="invoice", debtor_id="cust-2", amount=200.0)
    _txn(client, H2, transaction_type="invoice", debtor_id="cust-x", amount=999.0)
    _txn(client, {"X-User-Id": U1, "X-Book-ID": BOOK_B}, transaction_type="invoice", debtor_id="cust-b", amount=5.0)

    data = client.get("/debtors", headers=H1).json()
    assert {d["debtor_id"] for d in data["debtors"]} == {"cust-1", "cust-2"}
    assert data["total_balance"] == 300.0

    # other user sees only their own record; U1's Book B record stays out of Book A
    assert [d["debtor_id"] for d in client.get("/debtors", headers=H2).json()["debtors"]] == ["cust-x"]
    assert client.get("/debtors", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()["debtors"] == [
        {"debtor_id": "cust-b", "balance": 5.0}
    ]
    # personal scope sees the caller's own records across Books
    assert len(client.get("/debtors", headers=H1_PERSONAL).json()["debtors"]) == 3


def test_summary_and_reconcile():
    _txn(client, H1, transaction_type="invoice", amount=100.0)
    _txn(client, H1, transaction_type="credit_note", amount=10.0)
    _txn(client, H1, transaction_type="payment", amount=40.0)
    _txn(client, H1, transaction_type="bad_debt", amount=5.0)
    _txn(client, H2, transaction_type="invoice", debtor_id="cust-x", amount=999.0)

    s = client.get("/control-account/summary", headers=H1).json()
    assert s["total_invoices"] == 100.0
    assert s["total_credit_notes"] == 10.0
    assert s["total_payments"] == 40.0
    assert s["total_bad_debts"] == 5.0
    assert s["closing_balance"] == 45.0
    assert s["transaction_count"] == 4
    assert s["debtor_count"] == 1

    rec = client.post("/reconcile", headers=H1).json()
    # ledger total = 100 - 10 - 40 - 5 = 45 (refund-free ledger reconciles exactly)
    assert rec["control_account_balance"] == 45.0
    assert rec["sales_ledger_total"] == 45.0
    assert rec["reconciled"] is True


def test_cross_service_label_leak():
    """A :CreditorTransaction node (purchases twin) must never surface through sales endpoints."""
    _fake_session.nodes.append(
        {
            "label": "CreditorTransaction",
            "var": "n",
            "props": {
                "id": "foreign-1",
                "user_id": U1,
                "book_id": BOOK_A,
                "transaction_type": "purchase_invoice",
                "creditor_id": "cust-1",
                "creditor_name": "Not A Customer",
                "date": D,
                "amount": 777.0,
                "balance": 777.0,
            },
        }
    )
    data = client.get("/debtors", headers=H1).json()
    assert data["debtors"] == []
    assert data["total_balance"] == 0.0
