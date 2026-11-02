"""Book-scoping and arithmetic tests for retained-profits-service."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from retained_profits_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("retained_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "rp-user-1", "rp-user-2"
BOOK_A, BOOK_B = "rp-book-a", "rp-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

C1 = "company-1"


def _appropriate(company=C1, net=100000.0, opening=50000.0, allocs=None, headers=H1):
    return client.post(
        "/appropriations/create",
        params={
            "company_id": company,
            "financial_year": "FY2026",
            "period_start": "2026-01-01T00:00:00",
            "period_end": "2026-12-31T00:00:00",
            "net_profit": net,
            "opening_retained_earnings": opening,
        },
        json=allocs if allocs is not None else {"dividends": 30000.0, "general_reserve": 20000.0},
        headers=headers,
    )


class TestAppropriation:
    def test_the_statement_carries_the_balance_forward(self):
        body = _appropriate().json()
        assert body["total_appropriations"] == 50000.0
        assert body["balance_carried_forward"] == 50000.0
        assert body["closing_retained_earnings"] == 100000.0
        assert body["status"] == "approved"
        assert body["book_id"] == BOOK_A

    def test_the_named_allocations_round_trip(self):
        created = _appropriate().json()
        listed = client.get("/appropriations", params={"company_id": C1}, headers=H1).json()["appropriations"]
        assert [a["id"] for a in listed] == [created["id"]]
        assert listed[0]["appropriations"] == {"dividends": 30000.0, "general_reserve": 20000.0}

    def test_the_year_filter_reads_the_flag(self):
        _appropriate()
        found = client.get("/appropriations", params={"financial_year": "FY2026"}, headers=H1).json()
        assert len(found["appropriations"]) == 1
        none = client.get("/appropriations", params={"financial_year": "FY2019"}, headers=H1).json()
        assert none["appropriations"] == []

    def test_another_book_sees_nothing(self):
        _appropriate()
        assert client.get("/appropriations", headers=H1_BOOK_B).json()["appropriations"] == []


class TestDividends:
    def _declare(self, headers=H1, amount=12000.0):
        return client.post(
            "/interim-dividends/declare",
            params={
                "company_id": C1,
                "amount": amount,
                "per_share_amount": 1.2,
                "total_shares": 10000,
                "declaration_date": "2026-07-01T00:00:00",
            },
            headers=headers,
        )

    def test_a_declared_dividend_starts_unpaid(self):
        body = self._declare().json()
        assert body["status"] == "declared"
        assert body["payment_date"] is None
        assert body["amount"] == 12000.0

    def test_paying_it_stamps_the_date_and_status(self):
        created = self._declare().json()
        paid = client.post(
            f"/interim-dividends/{created['id']}/pay",
            params={"payment_date": "2026-08-01T00:00:00"},
            headers=H1,
        ).json()
        assert paid["status"] == "paid"
        assert paid["payment_date"].startswith("2026-08-01")
        # and the stored record agrees
        listed = client.get("/interim-dividends", headers=H1).json()["interim_dividends"]
        assert listed[0]["status"] == "paid"

    def test_paying_an_unknown_dividend_answers_two_hundred_with_an_error(self):
        """The original returns 200 {"error": ...} rather than a 404."""
        response = client.post(
            "/interim-dividends/ghost/pay", params={"payment_date": "2026-08-01T00:00:00"}, headers=H1
        )
        assert response.status_code == 200
        assert response.json() == {"error": "Dividend not found"}

    def test_another_user_cannot_pay_it(self):
        created = self._declare().json()
        body = client.post(
            f"/interim-dividends/{created['id']}/pay",
            params={"payment_date": "2026-08-01T00:00:00"},
            headers=H2,
        ).json()
        assert body == {"error": "Dividend not found"}
        listed = client.get("/interim-dividends", headers=H1).json()["interim_dividends"]
        assert listed[0]["status"] == "declared"


class TestAdjustments:
    def _adjust(self, amount=-5000.0, headers=H1):
        return client.post(
            "/adjustments/create",
            params={
                "company_id": C1,
                "adjustment_type": "correction_of_error",
                "description": "Stock overstated",
                "amount": amount,
                "affected_year": "FY2025",
                "adjustment_date": "2026-03-01T00:00:00",
            },
            headers=headers,
        )

    def test_a_negative_adjustment_is_stored_with_its_sign(self):
        body = self._adjust(amount=-5000.0).json()
        assert body["amount"] == -5000.0
        assert body["affected_year"] == "FY2025"

    def test_another_book_sees_no_adjustments(self):
        self._adjust()
        assert client.get("/adjustments", headers=H1_BOOK_B).json()["adjustments"] == []


class TestSummary:
    def test_the_summary_adds_adjustments_to_what_was_carried(self):
        _appropriate(net=100000.0, opening=50000.0)
        client.post(
            "/adjustments/create",
            params={
                "company_id": C1,
                "adjustment_type": "correction_of_error",
                "description": "Stock overstated",
                "amount": -5000.0,
                "affected_year": "FY2025",
            },
            headers=H1,
        )
        summary = client.get(f"/summary/{C1}", headers=H1).json()
        assert summary["total_carried_forward"] == 50000.0
        assert summary["total_prior_year_adjustments"] == -5000.0
        assert summary["net_retained_earnings"] == 45000.0
        assert summary["last_appropriation"]["financial_year"] == "FY2026"

    def test_the_last_appropriation_is_the_newest(self):
        _appropriate(net=100.0, opening=0.0)
        _appropriate(net=200.0, opening=0.0)
        summary = client.get(f"/summary/{C1}", headers=H1).json()
        assert summary["last_appropriation"]["net_profit"] == 200.0

    def test_another_users_summary_is_empty(self):
        _appropriate()
        summary = client.get(f"/summary/{C1}", headers=H2).json()
        assert summary["total_carried_forward"] == 0
        assert summary["last_appropriation"] is None
