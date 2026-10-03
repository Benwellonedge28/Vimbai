"""Book-scoping and persistence tests for purchases-ledger-control-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from purchases_ledger_control_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("plc_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "plc-user-1", "plc-user-2"
BOOK_A, BOOK_B = "plc-book-a", "plc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}

D = "2026-01-01T10:00:00"


def _txn(client, headers, **kw):
    params = {"creditor_id": "sup-1", "creditor_name": "Supplier One", "date": D, "amount": 100.0}
    params.update(kw)
    return client.post("/transactions", params=params, headers=headers)


def test_health_and_root():
    assert client.get("/health").json()["status"] == "healthy"
    assert "purchases-ledger-control" in client.get("/").json()["service"]


def test_transaction_balances_sequence():
    r1 = _txn(client, H1, transaction_type="purchase_invoice", amount=100.0)
    assert r1.status_code == 200, r1.text
    assert r1.json()["balance"] == 100.0

    r2 = _txn(client, H1, transaction_type="payment", amount=40.0)
    assert r2.json()["balance"] == 60.0
    r3 = _txn(client, H1, transaction_type="debit_note", amount=10.0)
    assert r3.json()["balance"] == 50.0
    r4 = _txn(client, H1, transaction_type="refund", amount=5.0)
    assert r4.json()["balance"] == 45.0
    # discount records the txn but does not move the balance (original semantics:
    # the txn's own stamped balance field stays 0, the creditor balance stays 45)
    r5 = _txn(client, H1, transaction_type="discount", amount=3.0)
    assert r5.status_code == 200
    cred = client.get("/creditors", headers=H1).json()
    assert cred["creditors"] == [{"creditor_id": "sup-1", "balance": 45.0}]

    # second creditor is independent
    r6 = _txn(client, H1, transaction_type="purchase_invoice", creditor_id="sup-2", creditor_name="S2", amount=200.0)
    assert r6.json()["balance"] == 200.0


def test_creditors_list_and_isolation():
    _txn(client, H1, transaction_type="purchase_invoice", amount=100.0)
    _txn(client, H1, transaction_type="purchase_invoice", creditor_id="sup-2", amount=200.0)
    _txn(client, H2, transaction_type="purchase_invoice", creditor_id="sup-x", amount=999.0)
    _txn(
        client,
        {"X-User-Id": U1, "X-Book-ID": BOOK_B},
        transaction_type="purchase_invoice",
        creditor_id="sup-b",
        amount=5.0,
    )

    data = client.get("/creditors", headers=H1).json()
    assert {c["creditor_id"] for c in data["creditors"]} == {"sup-1", "sup-2"}
    assert data["total_balance"] == 300.0

    # the other user sees only their own record; Book A of U1 excludes U1's Book B record
    assert [c["creditor_id"] for c in client.get("/creditors", headers=H2).json()["creditors"]] == ["sup-x"]
    assert client.get("/creditors", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()["creditors"] == [
        {"creditor_id": "sup-b", "balance": 5.0}
    ]
    # personal scope sees the caller's own records across Books
    assert len(client.get("/creditors", headers=H1_PERSONAL).json()["creditors"]) == 3


def test_summary_and_reconcile():
    _txn(client, H1, transaction_type="purchase_invoice", amount=100.0)
    _txn(client, H1, transaction_type="debit_note", amount=10.0)
    _txn(client, H1, transaction_type="payment", amount=40.0)
    _txn(client, H1, transaction_type="refund", amount=5.0)
    # other user's data must not leak into the summary
    _txn(client, H2, transaction_type="purchase_invoice", creditor_id="sup-x", amount=999.0)

    s = client.get("/control-account/summary", headers=H1).json()
    assert s["total_invoices"] == 100.0
    assert s["total_debit_notes"] == 10.0
    assert s["total_payments"] == 40.0
    # closing = invoices - debit_notes - payments (refund excluded, original formula)
    assert s["closing_balance"] == 50.0
    assert s["transaction_count"] == 4

    rec = client.post("/reconcile", headers=H1).json()
    # ledger total = 100 - 10 - 40 - 5 = 45; control = 50; difference 5 (original behavior)
    assert rec["control_account_balance"] == 50.0
    assert rec["purchases_ledger_total"] == 45.0
    assert rec["difference"] == 5.0
    assert rec["reconciled"] is False

    # clean ledger reconciles
    _txn(
        client, {"X-User-Id": "plc-clean", "X-Book-ID": "plc-book-c"}, transaction_type="purchase_invoice", amount=10.0
    )
    rec2 = client.post("/reconcile", headers={"X-User-Id": "plc-clean", "X-Book-ID": "plc-book-c"}).json()
    assert rec2["reconciled"] is True


def test_cross_service_label_leak():
    """A :DebtorTransaction node (sales twin) must never surface through purchases endpoints."""
    _fake_session.nodes.append(
        {
            "label": "DebtorTransaction",
            "var": "n",
            "props": {
                "id": "foreign-1",
                "user_id": U1,
                "book_id": BOOK_A,
                "transaction_type": "invoice",
                "debtor_id": "sup-1",
                "debtor_name": "Not A Supplier",
                "date": D,
                "amount": 777.0,
                "balance": 777.0,
            },
        }
    )
    data = client.get("/creditors", headers=H1).json()
    assert data["creditors"] == []
    assert data["total_balance"] == 0.0
