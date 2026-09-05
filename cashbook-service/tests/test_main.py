"""Cashbook Service tests — Neo4j-backed CRUD with Book scoping (fake harness)."""

import importlib.util
import os
from datetime import datetime, timezone

import main  # noqa: F401  (must come first: bootstraps the cashbook_service package)
from cashbook_service.database import Neo4jConnector
from fastapi.testclient import TestClient  # noqa: E402

client = TestClient(app := main.app)  # startup (real Neo4j) never runs

_spec = importlib.util.spec_from_file_location(
    "cashbook_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))


USER = "user-cashbook"
OTHER_USER = "user-other"
BOOK_A = "book-cash-a"
BOOK_B = "book-cash-b"

NOW = "2026-09-01T10:00:00+00:00"


def _headers(user=USER, book=None):
    h = {"X-User-Id": user}
    if book:
        h["X-Book-ID"] = book
    return h


def _account(code="CB-001", **over):
    payload = {
        "account_code": code,
        "account_name": f"Account {code}",
        "account_type": "cash",
        "currency": "USD",
        "opening_balance": "1000.00",
        "is_active": True,
        "reconciliation_enabled": True,
    }
    payload.update(over)
    return payload


def _entry(voucher="V-001", bank_account="CB-001", is_debit=True, amount="150.00", **over):
    payload = {
        "book_type": "receipts",
        "entry_date": NOW,
        "voucher_number": voucher,
        "description": "test entry",
        "account_code": "CB-001",
        "amount": amount,
        "is_debit": is_debit,
        "bank_account": bank_account,
        "posted_by": "teller",
    }
    payload.update(over)
    return payload


def _flow(category="sales", **over):
    payload = {
        "entry_date": NOW,
        "category": category,
        "description": "flow item",
        "expected_amount": "500.00",
        "actual_amount": "450.00",
        "cash_flow_type": "operating",
        "source": "cash_receipts",
    }
    payload.update(over)
    return payload


def _clear():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


class TestBankAccounts:
    def setup_method(self):
        _clear()

    def test_create_and_get_account(self):
        r = client.post("/accounts", json=_account(), headers=_headers())
        assert r.status_code == 200, r.text
        account_id = r.json()["id"]
        assert float(r.json()["current_balance"]) == 1000.00

        g = client.get(f"/accounts/{account_id}", headers=_headers())
        assert g.status_code == 200
        assert g.json()["account_code"] == "CB-001"

    def test_create_stamps_book_id(self):
        r = client.post("/accounts", json=_account(), headers=_headers(book=BOOK_A))
        assert r.status_code == 200
        assert r.json()["book_id"] == BOOK_A
        assert r.json()["user_id"] == USER

    def test_duplicate_account_code_conflict(self):
        assert client.post("/accounts", json=_account(), headers=_headers()).status_code == 200
        r = client.post("/accounts", json=_account(), headers=_headers())
        assert r.status_code == 409

    def test_list_accounts_filters(self):
        client.post("/accounts", json=_account("CB-001"), headers=_headers())
        client.post("/accounts", json=_account("CB-002", account_type="bank"), headers=_headers())

        r = client.get("/accounts", headers=_headers())
        assert len(r.json()) == 2

        r = client.get("/accounts", params={"account_type": "bank"}, headers=_headers())
        assert [a["account_code"] for a in r.json()] == ["CB-002"]

    def test_update_account(self):
        account_id = client.post("/accounts", json=_account(), headers=_headers()).json()["id"]
        r = client.put(f"/accounts/{account_id}", json=_account(account_name="Renamed"), headers=_headers())
        assert r.status_code == 200
        assert r.json()["account_name"] == "Renamed"

    def test_update_missing_account_404(self):
        r = client.put("/accounts/nope", json=_account(), headers=_headers())
        assert r.status_code == 404

    def test_book_isolation_accounts(self):
        a = client.post("/accounts", json=_account("CB-A"), headers=_headers(book=BOOK_A)).json()
        client.post("/accounts", json=_account("CB-B"), headers=_headers(book=BOOK_B))

        listed_a = client.get("/accounts", headers=_headers(book=BOOK_A)).json()
        codes = [x["account_code"] for x in listed_a]
        assert codes == ["CB-A"]

        # cross-Book direct read is invisible
        assert client.get(f"/accounts/{a['id']}", headers=_headers(book=BOOK_B)).status_code == 404

    def test_user_isolation(self):
        client.post("/accounts", json=_account(), headers=_headers())
        other = client.get("/accounts", headers=_headers(user=OTHER_USER)).json()
        assert other == []


class TestCashBookEntries:
    def setup_method(self):
        _clear()
        self.account_id = client.post("/accounts", json=_account(), headers=_headers()).json()["id"]

    def test_create_entry_updates_balance(self):
        r = client.post("/entries", json=_entry(), headers=_headers())
        assert r.status_code == 200, r.text
        assert float(r.json()["base_amount"]) == 150.00
        assert r.json()["status"] == "pending"

        acct = client.get(f"/accounts/{self.account_id}", headers=_headers()).json()
        assert float(acct["current_balance"]) == 1150.00

    def test_credit_entry_reduces_balance(self):
        client.post("/entries", json=_entry(is_debit=False), headers=_headers())
        acct = client.get(f"/accounts/{self.account_id}", headers=_headers()).json()
        assert float(acct["current_balance"]) == 850.00

    def test_post_and_void_entry(self):
        entry_id = client.post("/entries", json=_entry(), headers=_headers()).json()["id"]

        p = client.put(f"/entries/{entry_id}/post", params={"posted_by": "teller"}, headers=_headers())
        assert p.status_code == 200
        assert p.json()["status"] == "posted"

        v = client.put(
            f"/entries/{entry_id}/void", params={"voided_by": "teller", "reason": "mistake"}, headers=_headers()
        )
        assert v.status_code == 200
        assert v.json()["status"] == "voided"

        # balance reverted after void
        acct = client.get(f"/accounts/{self.account_id}", headers=_headers()).json()
        assert float(acct["current_balance"]) == 1000.00

    def test_entries_book_isolated(self):
        e_a = client.post("/entries", json=_entry("V-A", bank_account=None), headers=_headers(book=BOOK_A)).json()
        client.post("/entries", json=_entry("V-B", bank_account=None), headers=_headers(book=BOOK_B))

        listed_a = client.get("/entries", headers=_headers(book=BOOK_A)).json()
        assert [e["voucher_number"] for e in listed_a] == ["V-A"]

        assert client.get(f"/entries/{e_a['id']}", headers=_headers(book=BOOK_B)).status_code == 404


class TestSummary:
    def setup_method(self):
        _clear()
        client.post("/accounts", json=_account(), headers=_headers())
        client.post("/entries", json=_entry(), headers=_headers())
        client.post("/entries", json=_entry(is_debit=False), headers=_headers())
        e = client.post("/entries", json=_entry(voucher="V-2"), headers=_headers()).json()
        client.put(f"/entries/{e['id']}/post", params={"posted_by": "t"}, headers=_headers())

    def test_summary_only_counts_posted(self):
        r = client.get("/summary/CB-001", headers=_headers())
        assert r.status_code == 200
        data = r.json()
        assert data["transaction_count"] == 3
        assert float(data["total_debits"]) == 150.00  # only the posted one
        assert float(data["closing_balance"]) == 1150.00

    def test_summary_unknown_account_404(self):
        assert client.get("/summary/NOPE", headers=_headers()).status_code == 404


class TestReconciliations:
    def setup_method(self):
        _clear()
        client.post("/accounts", json=_account(), headers=_headers())
        e = client.post("/entries", json=_entry(), headers=_headers()).json()
        client.put(f"/entries/{e['id']}/post", params={"posted_by": "t"}, headers=_headers())

    def _recon(self, **over):
        payload = {
            "bank_account": "CB-001",
            "statement_date": NOW,
            "statement_balance": "1100.00",
            "prepared_by": "accountant",
        }
        payload.update(over)
        return payload

    def test_create_reconciliation_computes_balances(self):
        r = client.post("/reconciliations", json=self._recon(adjustments=[{"amount": "50.00"}]), headers=_headers())
        assert r.status_code == 200, r.text
        data = r.json()
        assert float(data["book_balance"]) == 1150.00  # opening 1000 + posted 150
        assert float(data["adjusted_balance"]) == 1150.00  # statement 1100 + adj 50
        assert data["status"] == "in_progress"

    def test_complete_reconciliation(self):
        entry_id = client.post("/entries", json=_entry(voucher="V-9"), headers=_headers()).json()["id"]
        recon_id = client.post(
            "/reconciliations",
            json=self._recon(adjustments=[{"entry_id": entry_id, "amount": "10.00"}]),
            headers=_headers(),
        ).json()["id"]

        r = client.put(f"/reconciliations/{recon_id}/complete", params={"reviewed_by": "boss"}, headers=_headers())
        assert r.status_code == 200
        assert r.json()["status"] == "completed"
        assert r.json()["reviewed_by"] == "boss"

        entry = client.get(f"/entries/{entry_id}", headers=_headers()).json()
        assert entry["reconciled"] is True
        assert entry["reconciliation_id"] == recon_id

    def test_outstanding_items(self):
        recon_id = client.post("/reconciliations", json=self._recon(), headers=_headers()).json()["id"]
        r = client.get(f"/reconciliations/{recon_id}/outstanding", headers=_headers())
        assert r.status_code == 200
        data = r.json()
        assert data["total_items"] == 1  # the one posted entry, unreconciled
        assert float(data["total_debits"]) == 150.00

    def test_reconciliations_book_isolated(self):
        r_a = client.post("/reconciliations", json=self._recon(), headers=_headers(book=BOOK_A)).json()
        client.post("/reconciliations", json=self._recon(), headers=_headers(book=BOOK_B))

        listed = client.get("/reconciliations", headers=_headers(book=BOOK_A)).json()
        assert len(listed) == 1

        assert client.get(f"/reconciliations/{r_a['id']}", headers=_headers(book=BOOK_B)).status_code == 404


class TestCashFlow:
    def setup_method(self):
        _clear()

    def test_create_flow_computes_variance(self):
        r = client.post("/cash-flow", json=_flow(), headers=_headers())
        assert r.status_code == 200, r.text
        assert float(r.json()["variance"]) == -50.00
        assert r.json()["book_id"] is None

    def test_list_and_summary(self):
        client.post("/cash-flow", json=_flow(), headers=_headers())
        client.post("/cash-flow", json=_flow(category="wages", cash_flow_type="financing"), headers=_headers())

        listed = client.get("/cash-flow", headers=_headers()).json()
        assert len(listed) == 2

        s = client.get(
            "/cash-flow/summary",
            params={"start_date": "2026-01-01T00:00:00+00:00", "end_date": "2026-12-31T00:00:00+00:00"},
            headers=_headers(),
        )
        assert s.status_code == 200, s.text
        assert s.json()["operating"]["count"] == 1
        assert s.json()["financing"]["count"] == 1

    def test_flow_book_isolated(self):
        client.post("/cash-flow", json=_flow(category="a"), headers=_headers(book=BOOK_A))
        client.post("/cash-flow", json=_flow(category="b"), headers=_headers(book=BOOK_B))

        listed_a = client.get("/cash-flow", headers=_headers(book=BOOK_A)).json()
        assert [x["category"] for x in listed_a] == ["a"]


class TestCashPosition:
    def setup_method(self):
        _clear()

    def test_position_aggregates(self):
        client.post("/accounts", json=_account("CB-001", account_type="cash"), headers=_headers())
        client.post("/accounts", json=_account("CB-002", account_type="bank"), headers=_headers())

        r = client.get("/cash-position", headers=_headers())
        assert r.status_code == 200
        data = r.json()
        assert float(data["total_cash"]) == 1000.00
        assert float(data["total_bank"]) == 1000.00
        assert len(data["by_account"]) == 2
        assert float(data["by_currency"]["USD"]) == 2000.00

    def test_position_history(self):
        client.post("/accounts", json=_account(), headers=_headers())
        client.post("/entries", json=_entry(), headers=_headers())

        r = client.get(
            "/cash-position/history",
            params={"start_date": "2026-01-01T00:00:00+00:00", "end_date": "2026-12-31T00:00:00+00:00"},
            headers=_headers(),
        )
        assert r.status_code == 200
        assert len(r.json()["history"]) == 1
        assert float(r.json()["history"][0]["running_balance"]) == 150.00

    def test_position_book_isolated(self):
        client.post("/accounts", json=_account("CB-001"), headers=_headers(book=BOOK_A))
        client.post("/accounts", json=_account("CB-002"), headers=_headers(book=BOOK_B))

        pos_a = client.get("/cash-position", headers=_headers(book=BOOK_A)).json()
        assert [a["account_code"] for a in pos_a["by_account"]] == ["CB-001"]
