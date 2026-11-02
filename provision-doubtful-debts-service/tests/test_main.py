"""Book-scoping, ageing and bug-pin tests for provision-doubtful-debts-service."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from provision_doubtful_debts_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("doubtful_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "dd-user-1", "dd-user-2"
BOOK_A, BOOK_B = "dd-book-a", "dd-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _add_debtor(debtor_id="d1", amount=1000.0, due="2026-02-01T00:00:00", risk="medium", headers=H1):
    return client.post(
        "/debtors/add",
        params={
            "debtor_id": debtor_id,
            "debtor_name": f"Debtor {debtor_id}",
            "invoice_number": f"INV-{debtor_id}",
            "invoice_date": "2026-01-01T00:00:00",
            "due_date": due,
            "amount": amount,
            "risk_assessment": risk,
        },
        headers=headers,
    )


class TestDebtors:
    def test_a_new_debtor_is_provided_at_its_risk_rate(self):
        body = _add_debtor(amount=1000.0, risk="high").json()
        assert body["provision_percent"] == 25
        assert body["provision_amount"] == 250.0
        assert body["book_id"] == BOOK_A

    def test_an_unrecognised_grade_falls_back_to_five_percent(self):
        body = _add_debtor(risk="wat").json()
        assert body["provision_percent"] == 5

    def test_the_risk_filter_reads_the_flag(self):
        _add_debtor(debtor_id="d1", risk="low")
        _add_debtor(debtor_id="d2", risk="high")
        high = client.get("/debtors", params={"risk_assessment": "high"}, headers=H1).json()
        assert [d["debtor_id"] for d in high["debtors"]] == ["d2"]
        assert client.get("/debtors", headers=H1).json()["count"] == 2

    def test_another_book_sees_nothing(self):
        _add_debtor()
        assert client.get("/debtors", headers=H1_BOOK_B).json()["count"] == 0


class TestAssessment:
    def test_an_assessment_resets_the_rate(self):
        _add_debtor(risk="medium")
        body = client.post(
            "/debtors/d1/assess",
            params={"risk_assessment": "doubtful", "notes": "gone quiet"},
            headers=H1,
        ).json()
        assert body["risk_assessment"] == "doubtful"
        assert body["provision_percent"] == 75
        assert body["provision_amount"] == 750.0
        assert body["notes"] == "gone quiet"
        # and it stuck
        stored = client.get("/debtors", headers=H1).json()["debtors"][0]
        assert stored["risk_assessment"] == "doubtful"

    def test_another_users_debtor_is_a_404(self):
        _add_debtor()
        response = client.post("/debtors/d1/assess", params={"risk_assessment": "high"}, headers=H2)
        assert response.status_code == 404
        # the owner's grade is untouched
        assert client.get("/debtors", headers=H1).json()["debtors"][0]["risk_assessment"] == "medium"


class TestAgeing:
    def test_ageing_escalates_by_days_overdue(self):
        _add_debtor(due="2027-01-01T00:00:00", risk="low")
        body = client.post("/calculate-provision", params={"period_end": "2027-06-30T00:00:00"}, headers=H1).json()
        assert body["entries"][0]["risk"] == "doubtful"
        assert body["entries"][0]["provision_percent"] == 75
        assert body["total_provision_required"] == 750.0

    def test_a_debt_within_sixty_days_keeps_its_grade(self):
        _add_debtor(due="2027-06-01T00:00:00", risk="low")
        body = client.post("/calculate-provision", params={"period_end": "2027-06-30T00:00:00"}, headers=H1).json()
        assert body["entries"][0]["risk"] == "low"
        assert body["entries"][0]["provision_percent"] == 1

    def test_escalation_never_relaxes_a_grade(self):
        _add_debtor(due="2027-06-01T00:00:00", risk="doubtful")
        body = client.post("/calculate-provision", params={"period_end": "2027-06-30T00:00:00"}, headers=H1).json()
        # 29 days overdue is inside the untouched band, so the 75% stands
        assert body["entries"][0]["provision_percent"] == 75

    def test_the_aged_figures_are_stored(self):
        _add_debtor(due="2027-01-01T00:00:00", risk="low")
        client.post("/calculate-provision", params={"period_end": "2027-06-30T00:00:00"}, headers=H1)
        stored = client.get("/debtors", headers=H1).json()["debtors"][0]
        assert stored["risk_assessment"] == "doubtful"
        # 2027-01-01 to 2027-06-30 is exactly 180 days, which is the doubtful band
        assert stored["days_overdue"] == 180


class TestProvisionRecord:
    def test_creating_a_provision_answers_200(self):
        """The original 500ed here: 5 validation errors for DoubtfulDebtProvision."""
        _add_debtor(amount=1000.0, due="2027-01-01T00:00:00", risk="low")
        response = client.post("/provision/create", params={"period_end": "2027-06-30T00:00:00"}, headers=H1)
        assert response.status_code == 200, response.text
        body = response.json()
        assert body["provision_required"] == 750.0
        assert body["provision_closing"] == 750.0
        # the record carries the full debtor entries, not the summary dicts
        assert body["entries"][0]["debtor_name"] == "Debtor d1"
        assert body["entries"][0]["invoice_number"] == "INV-d1"
        assert body["entries"][0]["due_date"].startswith("2027-01-01")

    def test_the_opening_figure_drives_the_adjustment(self):
        _add_debtor(amount=1000.0, due="2027-01-01T00:00:00")
        body = client.post(
            "/provision/create",
            params={"period_end": "2027-06-30T00:00:00", "provision_opening": 250.0},
            headers=H1,
        ).json()
        assert body["provision_adjustment"] == 500.0

    def test_history_returns_the_tail_oldest_first(self):
        _add_debtor(amount=100.0, due="2027-01-01T00:00:00")
        for _ in range(3):
            client.post("/provision/create", params={"period_end": "2027-06-30T00:00:00"}, headers=H1)
        history = client.get("/provisions", params={"limit": 2}, headers=H1).json()["provisions"]
        assert len(history) == 2
        assert history[0]["created_at"] <= history[1]["created_at"]

    def test_another_user_cannot_read_the_history(self):
        _add_debtor(amount=100.0, due="2027-01-01T00:00:00")
        client.post("/provision/create", params={"period_end": "2027-06-30T00:00:00"}, headers=H1)
        assert client.get("/provisions", headers=H2).json()["provisions"] == []
