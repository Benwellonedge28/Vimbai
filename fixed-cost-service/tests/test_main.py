"""Book-scoping and persistence tests for fixed-cost-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from fixed_cost_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("fixed_cost_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "fc-user-1", "fc-user-2"
BOOK_A, BOOK_B = "fc-book-a", "fc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

PERIOD = {"analysis_period_start": "2026-10-01T00:00:00", "analysis_period_end": "2026-10-31T00:00:00"}


def _add(amount=1000.0, code="RENT", committed=True, controllable=True, dept=None, headers=H1):
    params = {
        "cost_name": code.title(),
        "cost_code": code,
        "amount": amount,
        "committed_cost": committed,
        "controllable": controllable,
    }
    if dept:
        params["department_id"] = dept
    return client.post("/costs/add", params=params, headers=headers)


class TestCosts:
    def test_a_cost_round_trips(self):
        created = _add().json()
        assert created["cost_code"] == "RENT"
        assert created["book_id"] == BOOK_A
        listed = client.get("/costs", headers=H1).json()["fixed_costs"]
        assert [c["id"] for c in listed] == [created["id"]]
        assert listed[0]["amount"] == 1000.0
        assert listed[0]["committed_cost"] is True

    def test_update_writes_back(self):
        created = _add().json()
        updated = client.post(
            f"/costs/{created['id']}/update",
            params={"amount": 1250.0, "controllable": False},
            headers=H1,
        ).json()
        assert updated["amount"] == 1250.0
        assert updated["controllable"] is False
        assert client.get("/costs", headers=H1).json()["fixed_costs"][0]["amount"] == 1250.0

    def test_an_unknown_cost_reads_as_not_found(self):
        assert client.post("/costs/nope/update", params={"amount": 1}, headers=H1).json() == {"error": "Cost not found"}

    def test_another_user_cannot_update_it(self):
        created = _add().json()
        assert client.post(f"/costs/{created['id']}/update", params={"amount": 5}, headers=H2).json() == {
            "error": "Cost not found"
        }
        assert client.get("/costs", headers=H2).json()["fixed_costs"] == []

    def test_another_book_cannot_see_it(self):
        _add()
        assert client.get("/costs", headers=H1_BOOK_B).json()["fixed_costs"] == []
        assert client.get("/summary", headers=H1_BOOK_B).json()["total_cost_items"] == 0

    def test_the_committed_filter_reads_the_flag(self):
        _add(amount=1000, code="RENT", committed=True)
        _add(amount=250, code="TRAINING", committed=False)
        committed = client.get("/costs", params={"committed": True}, headers=H1).json()["fixed_costs"]
        assert [c["cost_code"] for c in committed] == ["RENT"]
        discretionary = client.get("/costs", params={"committed": False}, headers=H1).json()["fixed_costs"]
        assert [c["cost_code"] for c in discretionary] == ["TRAINING"]


class TestAnalysis:
    def test_the_analysis_splits_committed_and_controllable(self):
        _add(amount=1000, code="RENT", committed=True, controllable=False)
        _add(amount=250, code="TRAINING", committed=False, controllable=True)
        analysis = client.post("/analyze", params=PERIOD, headers=H1).json()
        assert analysis["total_fixed_costs"] == 1250.0
        assert analysis["committed_costs"] == 1000.0
        assert analysis["discretionary_costs"] == 250.0
        assert analysis["controllable_costs"] == 250.0
        assert analysis["non_controllable_costs"] == 1000.0
        assert len(analysis["fixed_cost_details"]) == 2

    def test_the_analysis_is_kept_and_can_be_listed(self):
        _add()
        analysis = client.post("/analyze", params=PERIOD, headers=H1).json()
        listed = client.get("/analyses", headers=H1).json()["analyses"]
        assert [a["id"] for a in listed] == [analysis["id"]]
        assert listed[0]["total_fixed_costs"] == 1000.0

    def test_analyses_can_be_narrowed_to_a_window(self):
        _add()
        client.post("/analyze", params=PERIOD, headers=H1)
        inside = client.get("/analyses", params={"period_start": "2026-10-15T00:00:00"}, headers=H1).json()
        assert inside["analyses"] == []
        outside = client.get("/analyses", params={"period_start": "2026-01-01T00:00:00"}, headers=H1).json()
        assert len(outside["analyses"]) == 1

    def test_an_analysis_only_counts_the_callers_costs(self):
        _add()
        _add(amount=999, code="OTHER", headers=H2)
        analysis = client.post("/analyze", params=PERIOD, headers=H1).json()
        assert analysis["total_fixed_costs"] == 1000.0

    def test_another_user_cannot_list_the_analysis(self):
        _add()
        client.post("/analyze", params=PERIOD, headers=H1)
        assert client.get("/analyses", headers=H2).json()["analyses"] == []


class TestSummary:
    def test_the_summary_totals_every_bucket(self):
        _add(amount=1000, code="RENT", committed=True, controllable=False)
        _add(amount=250, code="TRAINING", committed=False, controllable=True)
        summary = client.get("/summary", headers=H1).json()
        assert summary == {
            "total_fixed_costs": 1250.0,
            "committed_costs": 1000.0,
            "discretionary_costs": 250.0,
            "controllable_costs": 250.0,
            "non_controllable_costs": 1000.0,
            "total_cost_items": 2,
        }

    def test_the_summary_follows_a_department_filter(self):
        _add(amount=1000, code="RENT", dept="d1")
        _add(amount=250, code="TRAINING", dept="d2")
        summary = client.get("/summary", params={"department_id": "d1"}, headers=H1).json()
        assert summary["total_fixed_costs"] == 1000.0
        assert summary["total_cost_items"] == 1
