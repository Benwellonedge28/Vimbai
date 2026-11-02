"""Book-scoping and persistence tests for bad-debts-recovery-service."""

import importlib.util
import os

import main
import pytest
from bad_debts_recovery_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("bad_debts_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "bd-user-1", "bd-user-2"
BOOK_A, BOOK_B = "bd-book-a", "bd-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _register(debt_id="d1", written_off=1000.0, debtor="Acme", headers=H1):
    return client.post(
        "/bad-debts/register",
        params={
            "debt_id": debt_id,
            "debtor_name": debtor,
            "original_amount": written_off,
            "written_off_amount": written_off,
            "written_off_date": "2026-01-31T00:00:00",
        },
        headers=headers,
    )


def _recover(debt_id="d1", amount=400.0, headers=H1):
    return client.post(f"/bad-debts/{debt_id}/recover", params={"amount": amount}, headers=headers)


class TestRegistration:
    def test_a_registered_debt_is_pending_and_owned(self):
        created = _register().json()
        assert created["status"] == "pending"
        assert created["recovered_amount"] == 0
        assert created["book_id"] == BOOK_A
        listed = client.get("/bad-debts", headers=H1).json()
        assert listed["total"] == 1
        assert listed["records"][0]["original_debt_id"] == "d1"

    def test_it_can_be_read_back_by_its_debt_id(self):
        _register()
        fetched = client.get("/bad-debts/d1", headers=H1)
        assert fetched.status_code == 200
        assert fetched.json()["debtor_name"] == "Acme"

    def test_an_unknown_debt_is_a_404(self):
        assert client.get("/bad-debts/nope", headers=H1).status_code == 404

    def test_another_book_cannot_see_it(self):
        _register()
        assert client.get("/bad-debts", headers=H1_BOOK_B).json()["total"] == 0
        assert client.get("/bad-debts/d1", headers=H1_BOOK_B).status_code == 404


class TestRecovery:
    def test_a_partial_recovery_marks_the_debt_partial(self):
        _register(written_off=1000.0)
        body = _recover(amount=400.0).json()
        assert body["entry"]["recovered_amount"] == 400.0
        assert body["entry"]["status"] == "partial"
        assert body["recovery"]["amount"] == 400.0
        # the stored record agrees
        assert client.get("/bad-debts/d1", headers=H1).json()["recovered_amount"] == 400.0

    def test_recovering_the_full_amount_completes_it(self):
        _register(written_off=1000.0)
        _recover(amount=600.0)
        body = _recover(amount=400.0).json()
        assert body["entry"]["status"] == "completed"

    def test_a_completed_debt_cannot_be_recovered_again(self):
        _register(written_off=500.0)
        _recover(amount=500.0)
        assert _recover(amount=100.0).status_code == 400

    def test_recovering_an_unknown_debt_is_a_404(self):
        assert _recover(debt_id="ghost").status_code == 404

    def test_another_user_cannot_recover_it(self):
        _register()
        assert _recover(amount=100.0, headers=H2).status_code == 404
        # and the owner's figure is untouched
        assert client.get("/bad-debts/d1", headers=H1).json()["recovered_amount"] == 0

    def test_another_book_cannot_recover_it(self):
        _register()
        assert _recover(amount=100.0, headers=H1_BOOK_B).status_code == 404


class TestSummary:
    def test_the_summary_reports_the_recovery_rate(self):
        _register(debt_id="d1", written_off=1000.0)
        _register(debt_id="d2", written_off=1000.0, debtor="Beta")
        _recover(debt_id="d1", amount=400.0)
        summary = client.get("/recovery-summary", headers=H1).json()
        assert summary["total_bad_debts"] == 2
        assert summary["total_written_off"] == 2000.0
        assert summary["total_recovered"] == 400.0
        assert summary["pending_recovery"] == 1600.0
        assert summary["recovery_rate"] == 20.0
        assert summary["status_breakdown"] == {"pending": 1, "partial": 1, "completed": 0}

    def test_a_completed_debt_leaves_nothing_pending(self):
        _register(written_off=500.0)
        _recover(amount=500.0)
        summary = client.get("/recovery-summary", headers=H1).json()
        assert summary["pending_recovery"] == 0
        assert summary["recovery_rate"] == 100.0

    def test_the_summary_only_counts_the_callers_book(self):
        _register(written_off=1000.0)
        _register(debt_id="d9", written_off=5000.0, debtor="Other", headers=H2)
        summary = client.get("/recovery-summary", headers=H1).json()
        assert summary["total_bad_debts"] == 1
        assert summary["total_written_off"] == 1000.0


class TestFilter:
    def test_the_status_filter_reads_the_flag(self):
        _register(debt_id="d1", written_off=100.0)
        _register(debt_id="d2", written_off=100.0, debtor="Beta")
        _recover(debt_id="d1", amount=100.0)
        completed = client.get("/bad-debts", params={"status": "completed"}, headers=H1).json()
        assert [r["original_debt_id"] for r in completed["records"]] == ["d1"]
        pending = client.get("/bad-debts", params={"status": "pending"}, headers=H1).json()
        assert [r["original_debt_id"] for r in pending["records"]] == ["d2"]
