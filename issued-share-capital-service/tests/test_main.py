"""Issued Share Capital Service tests — Neo4j-backed, Book-scoped (fake harness)."""

import importlib.util
import os

import main  # noqa: F401  (must come first: bootstraps the package alias)
from fastapi.testclient import TestClient  # noqa: E402
from issued_share_capital_service.database import Neo4jConnector

client = TestClient(app := main.app)

_spec = importlib.util.spec_from_file_location(
    "isc_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

USER = "user-isc"
OTHER_USER = "user-other"
BOOK_A = "book-isc-a"
BOOK_B = "book-isc-b"


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
            "share_class": "ordinary",
            "shares_issued": 1000,
            "issue_price": 1.5,
            **params,
        },
        headers=headers or _headers(),
    )


class TestIssues:
    def setup_method(self):
        _clear()

    def test_issue_shares(self):
        r = _issue()
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["total_proceeds"] == 1500.0
        assert data["user_id"] == USER

    def test_issues_persist_per_company(self):
        _issue(company_id="co-1")
        _issue(company_id="co-1", shares_issued=500)
        _issue(company_id="co-2", shares_issued=100)

        assert len(client.get("/issues/co-1", headers=_headers()).json()["issues"]) == 2
        s = client.get("/summary/co-1", headers=_headers()).json()
        assert s["total_shares_issued"] == 1500
        assert s["total_proceeds"] == 2250.0

    def test_issue_book_isolated(self):
        _issue(company_id="co-a", headers=_headers(book=BOOK_A))
        _issue(company_id="co-b", headers=_headers(book=BOOK_B))

        a = client.get("/issues/co-a", headers=_headers(book=BOOK_A)).json()["issues"]
        assert len(a) == 1
        # BOOK_B cannot see BOOK_A's issue even for the same company id
        assert len(client.get("/issues/co-a", headers=_headers(book=BOOK_B)).json()["issues"]) == 0
        assert len(client.get("/issues/co-a", headers=_headers(user=OTHER_USER)).json()["issues"]) == 0

    def test_personal_view_spans_books(self):
        _issue(company_id="co-a", headers=_headers(book=BOOK_A))
        assert len(client.get("/issues/co-a", headers=_headers()).json()["issues"]) == 1


class TestShareholders:
    def setup_method(self):
        _clear()

    def test_register_and_list(self):
        r = client.post(
            "/shareholders/register",
            params={"company_id": "co-1", "name": "Tariro", "address": "Harare", "shares_held": 600},
            headers=_headers(),
        )
        assert r.status_code == 200, r.text
        assert r.json()["shares_held"] == 600

        listed = client.get("/shareholders/co-1", headers=_headers()).json()["shareholders"]
        assert len(listed) == 1
        assert listed[0]["name"] == "Tariro"

        # persisted across requests
        client.post(
            "/shareholders/register",
            params={"company_id": "co-1", "name": "Vimbai", "address": "Bulawayo", "shares_held": 400},
            headers=_headers(),
        )
        assert len(client.get("/shareholders/co-1", headers=_headers()).json()["shareholders"]) == 2

    def test_shareholders_book_isolated(self):
        client.post(
            "/shareholders/register",
            params={"company_id": "co-1", "name": "A", "address": "x"},
            headers=_headers(book=BOOK_A),
        )
        client.post(
            "/shareholders/register",
            params={"company_id": "co-1", "name": "B", "address": "y"},
            headers=_headers(book=BOOK_B),
        )
        a = client.get("/shareholders/co-1", headers=_headers(book=BOOK_A)).json()["shareholders"]
        assert len(a) == 1
        assert a[0]["name"] == "A"
        assert len(client.get("/shareholders/co-1", headers=_headers(user=OTHER_USER)).json()["shareholders"]) == 0
