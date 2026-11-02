"""Book-scoping and reconciliation tests for control-account-reconciliation-service."""

import importlib.util
import os

import main
import pytest
from control_account_reconciliation_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("car_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "car-user-1", "car-user-2"
BOOK_A, BOOK_B = "car-book-a", "car-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _report_error(amount=100.0, expected=90.0, account="sales_ledger", kind="amount_mismatch", headers=H1):
    return client.post(
        "/errors/report",
        json={
            "error_type": kind,
            "control_account_type": account,
            "description": "Posting to the wrong side",
            "amount": amount,
            "expected_amount": expected,
        },
        headers=headers,
    )


class TestErrors:
    def test_the_variance_is_the_absolute_gap(self):
        body = _report_error(amount=100.0, expected=90.0).json()
        assert body["variance"] == 10.0
        assert body["status"] == "pending"
        assert body["book_id"] == BOOK_A

    def test_a_negative_gap_still_reports_a_positive_variance(self):
        body = _report_error(amount=80.0, expected=100.0).json()
        assert body["variance"] == 20.0

    def test_the_listing_filters_by_status_and_type(self):
        _report_error()
        _report_error(kind="timing_difference")
        assert client.get("/errors", headers=H1).json()["count"] == 2
        by_type = client.get("/errors", params={"error_type": "timing_difference"}, headers=H1).json()
        assert by_type["count"] == 1
        assert client.get("/errors", params={"status": "resolved"}, headers=H1).json()["count"] == 0

    def test_errors_are_book_scoped(self):
        _report_error()
        assert client.get("/errors", headers=H1_BOOK_B).json()["count"] == 0
        assert client.get("/errors", headers=H2).json()["count"] == 0


class TestResolution:
    def test_a_resolution_closes_the_error(self):
        error = _report_error().json()
        body = client.post(
            f"/errors/{error['id']}/resolve",
            params={"resolution": "Reversed the duplicate", "resolved_by": "accountant-1"},
            headers=H1,
        ).json()
        assert body["status"] == "resolved"
        assert body["resolved_by"] == "accountant-1"
        assert body["resolved_at"] is not None

    def test_an_unknown_error_is_a_404(self):
        response = client.post("/errors/ghost/resolve", params={"resolution": "x", "resolved_by": "y"}, headers=H1)
        assert response.status_code == 404
        assert response.json()["detail"] == "Error not found"

    def test_another_user_cannot_resolve_it(self):
        error = _report_error().json()
        response = client.post(
            f"/errors/{error['id']}/resolve",
            params={"resolution": "x", "resolved_by": "y"},
            headers=H2,
        )
        assert response.status_code == 404


class TestReconcile:
    def test_a_matching_balance_reconciles(self):
        body = client.post(
            "/reconcile/sales_ledger",
            params={
                "as_of_date": "2026-10-31T00:00:00",
                "control_balance": 10000.0,
                "ledger_total": 10000.0,
            },
            headers=H1,
        ).json()
        assert body["difference"] == 0.0
        assert body["status"] == "reconciled"
        assert body["error_count"] == 0

    def test_a_gap_under_a_cent_still_reconciles(self):
        body = client.post(
            "/reconcile/bills_payable",
            params={
                "as_of_date": "2026-10-31T00:00:00",
                "control_balance": 1000.0,
                "ledger_total": 1000.004,
            },
            headers=H1,
        ).json()
        assert body["status"] == "reconciled"

    def test_a_gap_unreconciles_and_carries_the_pending_errors(self):
        _report_error(account="sales_ledger")
        body = client.post(
            "/reconcile/sales_ledger",
            params={
                "as_of_date": "2026-10-31T00:00:00",
                "control_balance": 10000.0,
                "ledger_total": 9500.0,
            },
            headers=H1,
        ).json()
        assert body["difference"] == 500.0
        assert body["status"] == "unreconciled"
        assert body["error_count"] == 1
        assert body["errors"][0]["control_account_type"] == "sales_ledger"

    def test_another_account_type_keeps_its_own_breaks(self):
        _report_error(account="sales_ledger")
        body = client.post(
            "/reconcile/purchases_ledger",
            params={
                "as_of_date": "2026-10-31T00:00:00",
                "control_balance": 10.0,
                "ledger_total": 10.0,
            },
            headers=H1,
        ).json()
        assert body["error_count"] == 0

    def test_a_resolved_error_is_not_counted(self):
        error = _report_error().json()
        client.post(
            f"/errors/{error['id']}/resolve",
            params={"resolution": "fixed", "resolved_by": "accountant"},
            headers=H1,
        )
        body = client.post(
            "/reconcile/sales_ledger",
            params={"as_of_date": "2026-10-31T00:00:00", "control_balance": 1.0, "ledger_total": 1.0},
            headers=H1,
        ).json()
        assert body["error_count"] == 0


class TestReportsAndSummary:
    def test_reports_keep_the_tail_up_to_the_limit(self):
        for balance in (1.0, 2.0, 3.0):
            client.post(
                "/reconcile/sales_ledger",
                params={
                    "as_of_date": "2026-10-31T00:00:00",
                    "control_balance": balance,
                    "ledger_total": 0.0,
                },
                headers=H1,
            )
        every = client.get("/reports", headers=H1).json()["reports"]
        assert [r["control_balance"] for r in every] == [1.0, 2.0, 3.0]
        tail = client.get("/reports", params={"limit": 2}, headers=H1).json()["reports"]
        assert [r["control_balance"] for r in tail] == [2.0, 3.0]

    def test_the_summary_counts_the_caller_only(self):
        _report_error(amount=100.0, expected=90.0)
        _report_error(amount=50.0, expected=45.0, kind="missing_entry")
        summary = client.get("/summary", headers=H1).json()
        assert summary["total_errors"] == 2
        assert summary["pending_errors"] == 2
        assert summary["total_variance"] == 15.0
        assert summary["by_type"]["amount_mismatch"] == 1
        assert summary["by_type"]["rounding_error"] == 0
        assert client.get("/summary", headers=H2).json()["total_errors"] == 0

    def test_reports_are_book_scoped(self):
        client.post(
            "/reconcile/sales_ledger",
            params={"as_of_date": "2026-10-31T00:00:00", "control_balance": 1.0, "ledger_total": 1.0},
            headers=H1,
        )
        assert client.get("/reports", headers=H1_BOOK_B).json()["reports"] == []
