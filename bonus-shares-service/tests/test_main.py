"""Bonus Shares Service tests — Neo4j-backed, Book-scoped (fake harness)."""

import importlib.util
import os

import main  # noqa: F401  (must come first: bootstraps the package alias)
from bonus_shares_service.database import Neo4jConnector
from fastapi.testclient import TestClient  # noqa: E402

client = TestClient(app := main.app)

_spec = importlib.util.spec_from_file_location(
    "bs_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

USER = "user-bon"
OTHER_USER = "user-other"
BOOK_A = "book-bon-a"
BOOK_B = "book-bon-b"


def _headers(user=USER, book=None):
    h = {"X-User-Id": user}
    if book:
        h["X-Book-ID"] = book
    return h


def _clear():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


def _issue(company_id="co-1", headers=None, **params):
    return client.post(
        "/issue",
        params={
            "company_id": company_id,
            "issue_date": "2026-09-01T10:00:00+00:00",
            "shares_issued": 1000,
            "nominal_value": 1.0,
            "source_reserve": "share_premium",
            **params,
        },
        json={"sh-1": 600, "sh-2": 400},
        headers=headers or _headers(),
    )


class TestBonusIssues:
    def setup_method(self):
        _clear()

    def test_issue_bonus_shares(self):
        r = _issue()
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["total_nominal_value"] == 1000.0
        assert data["amount_utilized"] == 1000.0
        assert data["shareholder_allocations"] == {"sh-1": 600, "sh-2": 400}
        assert data["user_id"] == USER

    def test_issues_persist(self):
        _issue(company_id="co-1")
        _issue(company_id="co-1", shares_issued=500)
        _issue(company_id="co-2")

        listed = client.get("/issues", params={"company_id": "co-1"}, headers=_headers()).json()["issues"]
        assert len(listed) == 2
        assert sum(i["shares_issued"] for i in listed) == 1500

    def test_book_isolation(self):
        _issue(company_id="co-a", headers=_headers(book=BOOK_A))
        _issue(company_id="co-a", headers=_headers(book=BOOK_B))

        a = client.get("/issues", params={"company_id": "co-a"}, headers=_headers(book=BOOK_A)).json()["issues"]
        assert len(a) == 1
        assert len(client.get("/issues", headers=_headers(book=BOOK_B)).json()["issues"]) == 1
        assert len(client.get("/issues", headers=_headers(user=OTHER_USER)).json()["issues"]) == 0

    def test_personal_view_spans_books(self):
        _issue(company_id="co-a", headers=_headers(book=BOOK_A))
        _issue(company_id="co-b", headers=_headers(book=BOOK_B))
        assert len(client.get("/issues", headers=_headers()).json()["issues"]) == 2
