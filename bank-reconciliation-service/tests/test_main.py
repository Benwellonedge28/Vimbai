"""Bank Reconciliation Service tests — Neo4j-backed CRUD with Book scoping (fake harness)."""

import importlib.util
import os

import main  # noqa: F401  (must come first: bootstraps the bank_reconciliation_service package)
from bank_reconciliation_service.database import Neo4jConnector
from fastapi.testclient import TestClient  # noqa: E402

client = TestClient(app := main.app)  # startup (real Neo4j) never runs

_spec = importlib.util.spec_from_file_location(
    "bankrec_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

USER = "user-bankrec"
OTHER_USER = "user-other"
BOOK_A = "book-rec-a"
BOOK_B = "book-rec-b"

NOW = "2026-09-01T10:00:00+00:00"


def _headers(user=USER, book=None):
    h = {"X-User-Id": user}
    if book:
        h["X-Book-ID"] = book
    return h


def _line(desc="Deposit from client", ref="REF-1", amount=500.0, is_debit=False, **over):
    payload = {
        "statement_id": "pending",
        "date": NOW,
        "description": desc,
        "reference": ref,
        "transaction_type": "deposit",
        "amount": amount,
        "balance": 1500.0,
        "is_debit": is_debit,
    }
    payload.update(over)
    return payload


def _statement(number="ST-001", lines=None, **over):
    payload = {
        "bank_account": "BANK-001",
        "statement_number": number,
        "statement_start_date": "2026-08-01T00:00:00+00:00",
        "statement_end_date": "2026-08-31T00:00:00+00:00",
        "opening_balance": 1000.0,
        "closing_balance": 1500.0,
        "lines": lines if lines is not None else [_line()],
    }
    payload.update(over)
    return payload


def _cash_entry(ref="REF-1", amount=500.0, desc="Deposit from client", is_debit=False, **over):
    payload = {
        "date": NOW,
        "description": desc,
        "reference": ref,
        "transaction_type": "deposit",
        "amount": amount,
        "is_debit": is_debit,
    }
    payload.update(over)
    return payload


def _clear():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


class TestStatements:
    def setup_method(self):
        _clear()

    def test_import_statement_computes_totals(self):
        r = client.post(
            "/statements",
            json=_statement(lines=[_line(amount=500.0), _line(desc="ATM", ref="REF-2", amount=100.0, is_debit=True)]),
            headers=_headers(),
        )
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["total_credits"] == 500.0
        assert data["total_debits"] == 100.0
        assert len(data["lines"]) == 2
        assert data["book_id"] is None
        assert data["user_id"] == USER

    def test_get_and_list_statements(self):
        sid = client.post("/statements", json=_statement(), headers=_headers()).json()["id"]
        client.post("/statements", json=_statement(number="ST-002", bank_account="BANK-002"), headers=_headers())

        g = client.get(f"/statements/{sid}", headers=_headers())
        assert g.status_code == 200
        assert g.json()["statement_number"] == "ST-001"

        listed = client.get("/statements", headers=_headers()).json()["statements"]
        assert len(listed) == 2

        by_acct = client.get("/statements", params={"bank_account": "BANK-002"}, headers=_headers()).json()
        assert len(by_acct["statements"]) == 1

    def test_missing_statement_404(self):
        assert client.get("/statements/nope", headers=_headers()).status_code == 404

    def test_statements_book_isolated(self):
        s_a = client.post("/statements", json=_statement(), headers=_headers(book=BOOK_A)).json()
        client.post("/statements", json=_statement(number="ST-B"), headers=_headers(book=BOOK_B))

        listed_a = client.get("/statements", headers=_headers(book=BOOK_A)).json()["statements"]
        assert [s["statement_number"] for s in listed_a] == ["ST-001"]

        assert client.get(f"/statements/{s_a['id']}", headers=_headers(book=BOOK_B)).status_code == 404

    def test_user_isolation(self):
        client.post("/statements", json=_statement(), headers=_headers())
        other = client.get("/statements", headers=_headers(user=OTHER_USER)).json()
        assert other["statements"] == []


class TestCashBookEntries:
    def setup_method(self):
        _clear()

    def test_add_entry(self):
        r = client.post("/cash-book", json=_cash_entry(), headers=_headers())
        assert r.status_code == 200, r.text
        assert r.json()["book_id"] is None
        assert r.json()["matched"] is False

    def test_add_entry_stamps_book(self):
        r = client.post("/cash-book", json=_cash_entry(ref="REF-X"), headers=_headers(book=BOOK_A))
        assert r.json()["book_id"] == BOOK_A

    def test_list_entries_with_filters(self):
        client.post("/cash-book", json=_cash_entry(ref="R1"), headers=_headers())
        client.post("/cash-book", json=_cash_entry(ref="R2", amount=250.0), headers=_headers())

        listed = client.get("/cash-book", params={"bank_account": "BANK-001"}, headers=_headers()).json()["entries"]
        assert len(listed) == 2

        matched_only = client.get(
            "/cash-book", params={"bank_account": "BANK-001", "matched": True}, headers=_headers()
        ).json()["entries"]
        assert matched_only == []

    def test_entries_book_isolated(self):
        client.post("/cash-book", json=_cash_entry(ref="RA"), headers=_headers(book=BOOK_A))
        client.post("/cash-book", json=_cash_entry(ref="RB"), headers=_headers(book=BOOK_B))

        listed_a = client.get("/cash-book", params={"bank_account": "BANK-001"}, headers=_headers(book=BOOK_A))
        assert [e["reference"] for e in listed_a.json()["entries"]] == ["RA"]


class TestReconciliation:
    def setup_method(self):
        _clear()
        self.statement_id = client.post("/statements", json=_statement(), headers=_headers()).json()["id"]
        self.entry_id = client.post("/cash-book", json=_cash_entry(), headers=_headers()).json()["id"]
        self.recon_id = client.post(
            "/reconcile",
            params={
                "bank_account": "BANK-001",
                "reconciliation_date": NOW,
                "statement_balance": 1500.0,
                "cash_book_balance": 1480.0,
                "bank_errors": 20.0,
            },
            headers=_headers(),
        ).json()["id"]

    def test_create_reconciliation_computes_adjustments(self):
        r = client.get(f"/reconciliations/{self.recon_id}", headers=_headers())
        assert r.status_code == 200
        data = r.json()
        assert data["adjusted_statement_balance"] == 1480.0  # 1500 - 0 + 0 - 20
        assert data["adjusted_cash_book_balance"] == 1460.0  # 1480 - 20 + 0
        assert data["difference"] == 20.0
        assert data["status"] == "in_progress"

    def test_auto_match(self):
        r = client.post(
            f"/reconcile/{self.recon_id}/auto-match", params={"statement_id": self.statement_id}, headers=_headers()
        )
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["matched_count"] == 1
        assert data["unmatched_bank"] == 0
        assert data["unmatched_cash_book"] == 0

        # persisted: entry now matched
        entries = client.get("/cash-book", params={"bank_account": "BANK-001"}, headers=_headers()).json()["entries"]
        assert entries[0]["matched"] is True

    def test_auto_match_missing_recon_404(self):
        r = client.post("/reconcile/nope/auto-match", params={"statement_id": self.statement_id}, headers=_headers())
        assert r.status_code == 404

    def test_post_adjustments_completes(self):
        r = client.post(f"/reconcile/{self.recon_id}/post-adjustments", headers=_headers())
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["status"] == "completed"
        assert data["completed_at"] is not None
        # accounting service unreachable in tests -> no journal id, tolerated
        assert data["journal_entry_id"] is None

    def test_reconciliations_book_isolated(self):
        r_a = client.post(
            "/reconcile",
            params={
                "bank_account": "BANK-001",
                "reconciliation_date": NOW,
                "statement_balance": 1.0,
                "cash_book_balance": 1.0,
            },
            headers=_headers(book=BOOK_A),
        ).json()
        client.post(
            "/reconcile",
            params={
                "bank_account": "BANK-001",
                "reconciliation_date": NOW,
                "statement_balance": 2.0,
                "cash_book_balance": 2.0,
            },
            headers=_headers(book=BOOK_B),
        )

        listed_a = client.get("/reconciliations", headers=_headers(book=BOOK_A)).json()["reconciliations"]
        assert len(listed_a) == 1

        assert client.get(f"/reconciliations/{r_a['id']}", headers=_headers(book=BOOK_B)).status_code == 404


class TestOutstandingItems:
    def setup_method(self):
        _clear()

    def test_no_reconciliations_returns_empty(self):
        r = client.get("/outstanding-items", params={"bank_account": "BANK-001"}, headers=_headers())
        assert r.status_code == 200
        assert r.json() == {
            "outstanding_cheques": [],
            "outstanding_deposits": [],
            "total_cheques": 0,
            "total_deposits": 0,
        }

    def test_outstanding_after_automatch(self):
        client.post(
            "/statements",
            json=_statement(lines=[_line(desc="Cheque 001", ref="CHQ-1", amount=300.0, is_debit=True)]),
            headers=_headers(),
        )
        client.post(
            "/cash-book",
            json=_cash_entry(ref="CHQ-1", amount=300.0, desc="Cheque 001", is_debit=True),
            headers=_headers(),
        )
        recon_id = client.post(
            "/reconcile",
            params={
                "bank_account": "BANK-001",
                "reconciliation_date": NOW,
                "statement_balance": 1500.0,
                "cash_book_balance": 1500.0,
            },
            headers=_headers(),
        ).json()["id"]
        # auto-match pairs them -> no outstanding items
        client.post("/reconcile", params={}, headers=_headers())  # no-op guard
        assert client.get(f"/reconciliations/{recon_id}", headers=_headers()).status_code == 200

        r = client.get("/outstanding-items", params={"bank_account": "BANK-001"}, headers=_headers())
        assert r.status_code == 200
        assert r.json()["outstanding_cheques"] == []
