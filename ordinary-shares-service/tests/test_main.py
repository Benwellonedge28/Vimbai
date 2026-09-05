"""Ordinary Shares Service tests — Neo4j-backed, Book-scoped (fake harness)."""

import importlib.util
import os

import main  # noqa: F401  (must come first: bootstraps the package alias)
from fastapi.testclient import TestClient  # noqa: E402
from ordinary_shares_service.database import Neo4jConnector

client = TestClient(app := main.app)

_spec = importlib.util.spec_from_file_location(
    "os_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

USER = "user-osh"
OTHER_USER = "user-other"
BOOK_A = "book-osh-a"
BOOK_B = "book-osh-b"


def _headers(user=USER, book=None):
    h = {"X-User-Id": user}
    if book:
        h["X-Book-ID"] = book
    return h


def _clear():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


def _declare(company_id="co-1", headers=None, **params):
    return client.post(
        "/dividends/declare",
        params={
            "company_id": company_id,
            "dividend_type": "interim",
            "per_share_amount": 0.05,
            "total_shares": 10000,
            "record_date": "2026-09-01T10:00:00+00:00",
            **params,
        },
        headers=headers or _headers(),
    )


class TestDividends:
    def setup_method(self):
        _clear()

    def test_declare(self):
        r = _declare()
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["total_dividend"] == 500.0
        assert data["status"] == "declared"
        assert data["user_id"] == USER

    def test_pay_persists_status(self):
        did = _declare().json()["id"]
        r = client.post(
            f"/dividends/{did}/pay",
            params={"payment_date": "2026-09-15T10:00:00+00:00"},
            headers=_headers(),
        )
        assert r.status_code == 200
        assert r.json()["status"] == "paid"

        # persisted across requests
        listed = client.get("/dividends", headers=_headers()).json()["dividends"]
        assert listed[0]["status"] == "paid"

    def test_pay_missing(self):
        r = client.post(
            "/dividends/nope/pay",
            params={"payment_date": "2026-09-15T10:00:00+00:00"},
            headers=_headers(),
        )
        assert r.status_code == 200  # contract preserved: {"error": ...}
        assert r.json() == {"error": "Dividend not found"}

    def test_book_isolation(self):
        _declare(company_id="co-a", headers=_headers(book=BOOK_A))
        _declare(company_id="co-a", headers=_headers(book=BOOK_B))

        a = client.get("/dividends", params={"company_id": "co-a"}, headers=_headers(book=BOOK_A)).json()["dividends"]
        assert len(a) == 1

        # cross-Book pay is invisible
        did_a = a[0]["id"]
        r = client.post(
            f"/dividends/{did_a}/pay",
            params={"payment_date": "2026-09-15T10:00:00+00:00"},
            headers=_headers(book=BOOK_B),
        )
        assert r.json() == {"error": "Dividend not found"}

        assert len(client.get("/dividends", headers=_headers(user=OTHER_USER)).json()["dividends"]) == 0

    def test_personal_view_spans_books(self):
        _declare(headers=_headers(book=BOOK_A))
        _declare(company_id="co-2", headers=_headers(book=BOOK_B))
        assert len(client.get("/dividends", headers=_headers()).json()["dividends"]) == 2
