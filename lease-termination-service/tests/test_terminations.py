"""Book-scoping and workflow tests for lease-termination-service."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from lease_termination_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("lt_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "lt-user-1", "lt-user-2"
BOOK_A, BOOK_B = "lt-book-a", "lt-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _create(lease_id="LEASE-1", remaining=6, amount=5000.0, penalty=2500.0, headers=H1):
    return client.post(
        "/terminations",
        params={
            "lease_id": lease_id,
            "termination_date": "2026-06-30T00:00:00",
            "original_end_date": "2027-06-30T00:00:00",
            "remaining_payments": remaining,
            "remaining_payment_amount": amount,
            "early_termination_penalty": penalty,
            "reason": "Premises closed",
        },
        headers=headers,
    )


class TestInitiation:
    def test_the_settlement_is_worked_out_at_initiation(self):
        body = _create().json()
        assert body["settlement_amount"] == 32500.0
        assert body["status"] == "pending"
        assert body["approved_by"] == ""
        assert body["book_id"] == BOOK_A

    def test_the_status_filter_reads_its_flag(self):
        first = _create(lease_id="L1").json()
        _create(lease_id="L2")
        client.put(f"/terminations/{first['id']}/approve", params={"approved_by": "cfo"}, headers=H1)
        approved = client.get("/terminations", params={"status": "approved"}, headers=H1).json()
        assert [t["lease_id"] for t in approved] == ["L1"]
        assert len(client.get("/terminations", headers=H1).json()) == 2

    def test_another_book_sees_nothing(self):
        created = _create().json()
        assert client.get("/terminations", headers=H1_BOOK_B).json() == []
        assert client.get(f"/terminations/{created['id']}", headers=H1_BOOK_B).status_code == 404


class TestSettlement:
    def test_net_settlement_nets_off_the_asset_return(self):
        created = _create().json()
        body = client.post(
            f"/terminations/{created['id']}/settlement",
            params={"asset_return_value": 4000.0},
            headers=H1,
        ).json()
        assert body["remaining_lease_payments"] == 30000.0
        assert body["early_termination_penalty"] == 2500.0
        assert body["net_settlement"] == 28500.0

    def test_repeated_calculations_are_kept_separately(self):
        created = _create().json()
        client.post(f"/terminations/{created['id']}/settlement", params={"asset_return_value": 0.0}, headers=H1)
        second = client.post(
            f"/terminations/{created['id']}/settlement",
            params={"asset_return_value": 500.0},
            headers=H1,
        ).json()
        assert second["net_settlement"] == 32000.0

    def test_a_settlement_can_be_calculated_before_approval(self):
        created = _create().json()
        response = client.post(f"/terminations/{created['id']}/settlement", headers=H1)
        assert response.status_code == 200

    def test_an_unknown_termination_has_no_settlement(self):
        assert client.post("/terminations/ghost/settlement", headers=H1).status_code == 404


class TestWorkflow:
    def test_approve_then_complete(self):
        created = _create().json()
        approved = client.put(
            f"/terminations/{created['id']}/approve", params={"approved_by": "cfo"}, headers=H1
        ).json()
        assert approved["status"] == "approved"
        assert client.get(f"/terminations/{created['id']}", headers=H1).json()["approved_by"] == "cfo"

        completed = client.put(f"/terminations/{created['id']}/complete", headers=H1).json()
        assert completed["status"] == "completed"
        stored = client.get(f"/terminations/{created['id']}", headers=H1).json()
        assert stored["status"] == "completed"
        assert stored["completed_at"] is not None

    def test_a_second_approval_is_a_400(self):
        created = _create().json()
        client.put(f"/terminations/{created['id']}/approve", params={"approved_by": "cfo"}, headers=H1)
        response = client.put(f"/terminations/{created['id']}/approve", params={"approved_by": "ceo"}, headers=H1)
        assert response.status_code == 400
        assert response.json()["detail"] == "Termination is not pending"

    def test_completion_before_approval_is_a_400(self):
        created = _create().json()
        response = client.put(f"/terminations/{created['id']}/complete", headers=H1)
        assert response.status_code == 400
        assert response.json()["detail"] == "Termination must be approved first"

    def test_an_unknown_termination_cannot_move(self):
        assert client.put("/terminations/ghost/approve", params={"approved_by": "cfo"}, headers=H1).status_code == 404
        assert client.put("/terminations/ghost/complete", headers=H1).status_code == 404

    def test_another_user_cannot_read_or_advance_it(self):
        created = _create().json()
        assert client.get(f"/terminations/{created['id']}", headers=H2).status_code == 404
        assert (
            client.put(f"/terminations/{created['id']}/approve", params={"approved_by": "x"}, headers=H2).status_code
            == 404
        )
        assert client.get(f"/terminations/{created['id']}", headers=H1).json()["status"] == "pending"
