"""Book-scoping and persistence tests for provision-bad-debts-service."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from provision_bad_debts_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("provision_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "pd-user-1", "pd-user-2"
BOOK_A, BOOK_B = "pd-book-a", "pd-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

PERIOD_END = "2026-10-31T00:00:00"


def _add_debtor(debtor_id="d1", amount=1000.0, name="Acme", headers=H1):
    return client.post(
        "/debtors/add",
        params={
            "debtor_id": debtor_id,
            "debtor_name": name,
            "invoice_number": f"INV-{debtor_id}",
            "original_amount": amount,
            "invoice_date": "2026-01-01T00:00:00",
        },
        headers=headers,
    )


class TestPolicies:
    def test_a_policy_round_trips_with_its_rules(self):
        created = client.post(
            "/policies",
            json={
                "name": "Ageing policy",
                "method": "ageing_analysis",
                "policy_rules": {"30": 2, "60": 8, "90": 20, "180": 60},
                "default_provision_percent": 0,
                "is_active": True,
            },
            headers=H1,
        )
        assert created.status_code == 201
        body = created.json()
        assert body["policy_rules"]["90"] == 20
        assert body["book_id"] == BOOK_A
        listed = client.get("/policies", headers=H1).json()["policies"]
        assert [p["id"] for p in listed] == [body["id"]]
        assert listed[0]["policy_rules"]["180"] == 60

    def test_the_active_filter_reads_the_flag(self):
        for active in (True, False):
            client.post(
                "/policies",
                json={"name": f"p{active}", "method": "ageing_analysis", "is_active": active},
                headers=H1,
            )
        only_active = client.get("/policies", params={"is_active": True}, headers=H1).json()["policies"]
        assert len(only_active) == 1
        assert only_active[0]["is_active"] is True

    def test_another_book_cannot_see_the_policy(self):
        client.post(
            "/policies",
            json={"name": "hidden", "method": "ageing_analysis", "is_active": True},
            headers=H1,
        )
        assert client.get("/policies", headers=H1_BOOK_B).json()["policies"] == []


class TestCalculation:
    def test_a_fresh_debtor_sits_in_the_current_bucket(self):
        _add_debtor(amount=1000.0)
        body = client.post("/calculate-provision", params={"period_end": PERIOD_END}, headers=H1).json()
        assert body["provision_required"] == 0
        assert body["entries"][0]["status"] == "current"
        assert body["ageing_summary"]["current"] == 1000.0

    def test_the_worked_out_figures_are_stored_on_the_debtor(self):
        _add_debtor(amount=1000.0)
        client.post("/calculate-provision", params={"period_end": PERIOD_END}, headers=H1)
        # a second ageing pass reads what the first one wrote back
        second = client.post("/calculate-provision", params={"period_end": PERIOD_END}, headers=H1).json()
        assert second["entries"][0]["status"] == "current"
        assert second["entries"][0]["provision_percent"] == 0

    def test_a_long_overdue_debtor_is_provided_at_the_policy_rate(self):
        _add_debtor(amount=1000.0)
        # age the debt well past 180 days, then provide against a policy
        policy = client.post(
            "/policies",
            json={
                "name": "harsh",
                "method": "ageing_analysis",
                "policy_rules": {"30": 2, "60": 8, "90": 20, "180": 65},
                "is_active": True,
            },
            headers=H1,
        ).json()
        distant = "2027-06-30T00:00:00"
        body = client.post(
            "/calculate-provision",
            params={"period_end": distant, "policy_id": policy["id"]},
            headers=H1,
        ).json()
        assert body["entries"][0]["status"] == "overdue_180"
        assert body["entries"][0]["provision_percent"] == 65
        assert body["provision_required"] == 650.0

    def test_the_default_ladder_applies_when_no_policy_is_given(self):
        _add_debtor(amount=1000.0)
        distant = "2027-06-30T00:00:00"
        body = client.post("/calculate-provision", params={"period_end": distant}, headers=H1).json()
        # 180 days or more defaults to 50%
        assert body["entries"][0]["provision_percent"] == 50
        assert body["provision_required"] == 500.0

    def test_only_the_callers_debtors_are_provided_for(self):
        _add_debtor(amount=1000.0)
        _add_debtor(debtor_id="d9", amount=5000.0, name="Other", headers=H2)
        body = client.post("/calculate-provision", params={"period_end": PERIOD_END}, headers=H1).json()
        assert len(body["entries"]) == 1
        assert body["total_debtors"] == 1000.0


class TestJournal:
    def test_the_journal_records_the_adjustment(self):
        _add_debtor(amount=1000.0)
        body = client.post(
            "/journal/create",
            params={"period_end": "2027-06-30T00:00:00", "total_debtors": 1000.0, "provision_opening": 200.0},
            headers=H1,
        ).json()
        assert body["provision_required"] == 500.0
        assert body["provision_adjustment"] == 300.0
        assert body["provision_closing"] == 500.0
        assert body["provision_opening"] == 200.0

    def test_history_returns_the_tail_oldest_first(self):
        _add_debtor(amount=100.0)
        for _ in range(3):
            client.post(
                "/journal/create",
                params={"period_end": PERIOD_END, "total_debtors": 100.0},
                headers=H1,
            )
        history = client.get("/provision-history", params={"limit": 2}, headers=H1).json()["journals"]
        assert len(history) == 2
        # the oldest of the tail comes first
        assert history[0]["created_at"] <= history[1]["created_at"]

    def test_another_user_cannot_read_the_history(self):
        _add_debtor(amount=100.0)
        client.post("/journal/create", params={"period_end": PERIOD_END, "total_debtors": 100.0}, headers=H1)
        assert client.get("/provision-history", headers=H2).json()["journals"] == []
