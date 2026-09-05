"""Authorized Share Capital Service tests — Neo4j-backed, Book-scoped (fake harness)."""

import importlib.util
import os

import main  # noqa: F401  (must come first: bootstraps the package alias)
from authorized_share_capital_service.database import Neo4jConnector
from fastapi.testclient import TestClient  # noqa: E402

client = TestClient(app := main.app)

_spec = importlib.util.spec_from_file_location(
    "asc_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

USER = "user-asc"
OTHER_USER = "user-other"
BOOK_A = "book-asc-a"
BOOK_B = "book-asc-b"


def _headers(user=USER, book=None):
    h = {"X-User-Id": user}
    if book:
        h["X-Book-ID"] = book
    return h


def _clear():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


def _create_class(name="ordinary", authorized=1000000, headers=None, **params):
    return client.post(
        "/share-classes",
        params={"name": name, "authorized_shares": authorized, **params},
        headers=headers or _headers(),
    )


class TestShareClasses:
    def setup_method(self):
        _clear()

    def test_create_and_list_class(self):
        r = _create_class()
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["name"] == "ordinary"
        assert data["authorized_shares"] == 1000000
        assert data["issued_shares"] == 0
        assert data["user_id"] == USER

        listed = client.get("/share-classes", headers=_headers())
        assert listed.status_code == 200
        assert len(listed.json()) == 1

    def test_invalid_voting_rights(self):
        r = _create_class(voting_rights="super")
        assert r.status_code == 400

    def test_class_book_isolated(self):
        _create_class(headers=_headers(book=BOOK_A))
        _create_class(name="founder", authorized=500, headers=_headers(book=BOOK_B))

        a = client.get("/share-classes", headers=_headers(book=BOOK_A)).json()
        assert len(a) == 1
        assert a[0]["name"] == "ordinary"

        assert len(client.get("/share-classes", headers=_headers(user=OTHER_USER)).json()) == 0

    def test_personal_view_spans_books(self):
        _create_class(headers=_headers(book=BOOK_A))
        _create_class(name="founder", authorized=500, headers=_headers(book=BOOK_B))
        assert len(client.get("/share-classes", headers=_headers()).json()) == 2


class TestIssuance:
    def setup_method(self):
        _clear()
        _create_class()

    def test_issue_within_authorized(self):
        class_id = client.get("/share-classes", headers=_headers()).json()[0]["id"]
        r = client.post(
            f"/share-classes/{class_id}/issue",
            params={"number_of_shares": 100000, "issue_price": 1.50, "issued_to": "Tariro"},
            headers=_headers(),
        )
        assert r.status_code == 200, r.text
        assert r.json()["total_proceeds"] == 150000.0

        # issued count persisted on the class
        classes = client.get("/share-classes", headers=_headers()).json()
        assert classes[0]["issued_shares"] == 100000

        issuances = client.get(f"/share-classes/{class_id}/issuances", headers=_headers()).json()
        assert len(issuances) == 1
        assert issuances[0]["issued_to"] == "Tariro"

    def test_issue_exceeds_authorized(self):
        class_id = client.get("/share-classes", headers=_headers()).json()[0]["id"]
        r = client.post(
            f"/share-classes/{class_id}/issue",
            params={"number_of_shares": 2000000, "issue_price": 1.0},
            headers=_headers(),
        )
        assert r.status_code == 400

    def test_issue_class_404(self):
        r = client.post(
            "/share-classes/nope/issue",
            params={"number_of_shares": 1, "issue_price": 1.0},
            headers=_headers(),
        )
        assert r.status_code == 404

    def test_issuance_book_isolated(self):
        _create_class(headers=_headers(book=BOOK_A))
        class_a = client.get("/share-classes", headers=_headers(book=BOOK_A)).json()[0]["id"]
        client.post(
            f"/share-classes/{class_a}/issue",
            params={"number_of_shares": 100, "issue_price": 2.0},
            headers=_headers(book=BOOK_A),
        )
        # class visible only in BOOK_A; issuances too
        issuances_a = client.get(f"/share-classes/{class_a}/issuances", headers=_headers(book=BOOK_A)).json()
        assert len(issuances_a) == 1
        # a different Book cannot issue against BOOK_A's class
        r = client.post(
            f"/share-classes/{class_a}/issue",
            params={"number_of_shares": 10, "issue_price": 2.0},
            headers=_headers(book=BOOK_B),
        )
        assert r.status_code == 404


class TestBuyback:
    def setup_method(self):
        _clear()
        _create_class()
        class_id = client.get("/share-classes", headers=_headers()).json()[0]["id"]
        client.post(
            f"/share-classes/{class_id}/issue",
            params={"number_of_shares": 1000, "issue_price": 1.0},
            headers=_headers(),
        )
        self.class_id = class_id

    def test_buyback_within_issued(self):
        r = client.post(
            f"/share-classes/{self.class_id}/buyback",
            params={"number_of_shares": 400, "buyback_price": 1.25},
            headers=_headers(),
        )
        assert r.status_code == 200, r.text
        assert r.json()["total_cost"] == 500.0

        # issued count reduced and persisted
        classes = client.get("/share-classes", headers=_headers()).json()
        assert classes[0]["issued_shares"] == 600

    def test_buyback_exceeds_issued(self):
        r = client.post(
            f"/share-classes/{self.class_id}/buyback",
            params={"number_of_shares": 99999, "buyback_price": 1.0},
            headers=_headers(),
        )
        assert r.status_code == 400

    def test_buyback_404(self):
        r = client.post(
            "/share-classes/nope/buyback",
            params={"number_of_shares": 1, "buyback_price": 1.0},
            headers=_headers(),
        )
        assert r.status_code == 404


class TestSummary:
    def setup_method(self):
        _clear()
        _create_class(name="ordinary", authorized=10000)
        _create_class(name="preference", authorized=5000, dividend_rate=8.0)
        class_ids = [c["id"] for c in client.get("/share-classes", headers=_headers()).json()]
        client.post(
            f"/share-classes/{class_ids[0]}/issue",
            params={"number_of_shares": 3000, "issue_price": 2.0},
            headers=_headers(),
        )
        client.post(
            f"/share-classes/{class_ids[1]}/issue",
            params={"number_of_shares": 1000, "issue_price": 1.0},
            headers=_headers(),
        )
        client.post(
            f"/share-classes/{class_ids[0]}/buyback",
            params={"number_of_shares": 500, "buyback_price": 2.5},
            headers=_headers(),
        )

    def test_summary_totals(self):
        s = client.get("/summary", headers=_headers()).json()
        assert s["total_classes"] == 2
        assert s["total_authorized"] == 15000
        assert s["total_issued"] == 3500  # 3000-500 + 1000
        assert s["total_proceeds"] == 7000.0
        assert s["total_buyback_cost"] == 1250.0
        by_name = {row["name"]: row for row in s["by_class"]}
        assert by_name["ordinary"]["available"] == 7500

    def test_summary_book_isolated(self):
        _create_class(name="founder", authorized=777, headers=_headers(book=BOOK_B))
        s_b = client.get("/summary", headers=_headers(book=BOOK_B)).json()
        assert s_b["total_classes"] == 1
        assert s_b["total_authorized"] == 777
        # personal view spans Books
        s_none = client.get("/summary", headers=_headers()).json()
        assert s_none["total_classes"] == 3
