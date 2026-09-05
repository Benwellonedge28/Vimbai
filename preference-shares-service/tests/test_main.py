"""Preference Shares Service tests — Neo4j-backed, Book-scoped (fake harness)."""

import importlib.util
import os

import main  # noqa: F401  (must come first: bootstraps the package alias)
from fastapi.testclient import TestClient  # noqa: E402
from preference_shares_service.database import Neo4jConnector

client = TestClient(app := main.app)

_spec = importlib.util.spec_from_file_location(
    "psh_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

USER = "user-psh"
OTHER_USER = "user-other"
BOOK_A = "book-psh-a"
BOOK_B = "book-psh-b"


def _headers(user=USER, book=None):
    h = {"X-User-Id": user}
    if book:
        h["X-Book-ID"] = book
    return h


def _clear():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


def _create_class(company_id="co-1", headers=None, **params):
    return client.post(
        "/classes/create",
        params={
            "name": "Series A",
            "company_id": company_id,
            "nominal_value": 1.0,
            "issue_price": 1.5,
            "fixed_dividend_rate": 8.0,
            "dividend_type": "cumulative",
            "participation_rights": "none",
            "liquidation_priority": 1,
            **params,
        },
        headers=headers or _headers(),
    )


def _class_id(company_id="co-1", headers=None):
    classes = client.get("/classes", params={"company_id": company_id}, headers=headers or _headers()).json()
    return classes["share_classes"][0]["id"]


class TestShareClasses:
    def setup_method(self):
        _clear()

    def test_create_and_list(self):
        r = _create_class()
        assert r.status_code == 200, r.text
        assert r.json()["shares_issued"] == 0
        assert r.json()["user_id"] == USER
        assert len(client.get("/classes", headers=_headers()).json()["share_classes"]) == 1

    def test_book_isolated(self):
        _create_class(headers=_headers(book=BOOK_A))
        _create_class(name="Series B", headers=_headers(book=BOOK_B))
        a = client.get("/classes", headers=_headers(book=BOOK_A)).json()["share_classes"]
        assert len(a) == 1
        assert a[0]["name"] == "Series A"
        assert len(client.get("/classes", headers=_headers(user=OTHER_USER)).json()["share_classes"]) == 0

    def test_personal_view_spans_books(self):
        _create_class(headers=_headers(book=BOOK_A))
        _create_class(company_id="co-2", headers=_headers(book=BOOK_B))
        assert len(client.get("/classes", headers=_headers()).json()["share_classes"]) == 2


class TestIssueAndRedeem:
    def setup_method(self):
        _clear()
        _create_class()

    def test_issue_updates_counts_and_persists(self):
        cid = _class_id()
        r = client.post(
            f"/classes/{cid}/issue",
            params={"shares_issued": 5000, "issue_date": "2026-09-01T10:00:00+00:00"},
            headers=_headers(),
        )
        assert r.status_code == 200
        assert r.json()["shares_issued"] == 5000
        assert r.json()["shares_outstanding"] == 5000

        # persisted
        classes = client.get("/classes", headers=_headers()).json()["share_classes"]
        assert classes[0]["shares_issued"] == 5000

    def test_issue_missing_class(self):
        r = client.post(
            "/classes/nope/issue",
            params={"shares_issued": 100, "issue_date": "2026-09-01T10:00:00+00:00"},
            headers=_headers(),
        )
        assert r.json() == {"error": "Share class not found"}

    def test_redeem_updates_outstanding(self):
        cid = _class_id()
        client.post(
            f"/classes/{cid}/issue",
            params={"shares_issued": 5000, "issue_date": "2026-09-01T10:00:00+00:00"},
            headers=_headers(),
        )
        r = client.post(
            f"/classes/{cid}/redeem",
            params={
                "shares_redeemed": 1000,
                "redemption_price": 1.6,
                "redemption_date": "2026-09-10T10:00:00+00:00",
            },
            headers=_headers(),
        )
        assert r.status_code == 200
        assert r.json()["total_proceeds"] == 1600.0

        classes = client.get("/classes", headers=_headers()).json()["share_classes"]
        assert classes[0]["shares_outstanding"] == 4000

    def test_cross_book_redeem_invisible(self):
        _create_class(headers=_headers(book=BOOK_A))
        cid = _class_id(headers=_headers(book=BOOK_A))
        client.post(
            f"/classes/{cid}/issue",
            params={"shares_issued": 1000, "issue_date": "2026-09-01T10:00:00+00:00"},
            headers=_headers(book=BOOK_A),
        )
        # BOOK_B cannot redeem BOOK_A's class
        r = client.post(
            f"/classes/{cid}/redeem",
            params={
                "shares_redeemed": 100,
                "redemption_price": 1.6,
                "redemption_date": "2026-09-10T10:00:00+00:00",
            },
            headers=_headers(book=BOOK_B),
        )
        assert r.json() == {"error": "Share class not found"}


class TestDividends:
    def setup_method(self):
        _clear()
        _create_class()

    def test_declare_and_pay(self):
        cid = _class_id()
        r = client.post(
            f"/classes/{cid}/dividends/declare",
            params={
                "company_id": "co-1",
                "per_share_amount": 0.08,
                "total_shares": 10000,
                "preference_arears": 200.0,
                "record_date": "2026-09-01T10:00:00+00:00",
            },
            headers=_headers(),
        )
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["total_dividend"] == 1000.0  # 0.08*10000 + 200
        assert data["status"] == "declared"

        did = data["id"]
        r2 = client.post(
            f"/classes/{cid}/dividends/{did}/pay",
            params={"payment_date": "2026-09-15T10:00:00+00:00"},
            headers=_headers(),
        )
        assert r2.json()["status"] == "paid"

        # persists
        divs = client.get("/dividends", headers=_headers()).json()["dividends"]
        assert divs[0]["status"] == "paid"

    def test_cross_book_pay_invisible(self):
        _create_class(headers=_headers(book=BOOK_A))
        cid = _class_id(headers=_headers(book=BOOK_A))
        d = client.post(
            f"/classes/{cid}/dividends/declare",
            params={
                "company_id": "co-1",
                "per_share_amount": 0.05,
                "total_shares": 1000,
                "preference_arears": 0,
                "record_date": "2026-09-01T10:00:00+00:00",
            },
            headers=_headers(book=BOOK_A),
        ).json()
        r = client.post(
            f"/classes/{cid}/dividends/{d['id']}/pay",
            params={"payment_date": "2026-09-15T10:00:00+00:00"},
            headers=_headers(book=BOOK_B),
        )
        assert r.json() == {"error": "Dividend not found"}
