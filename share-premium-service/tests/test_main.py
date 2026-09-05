"""Share Premium Service tests — Neo4j-backed, Book-scoped (fake harness)."""

import importlib.util
import os

import main  # noqa: F401  (must come first: bootstraps the package alias)
from fastapi.testclient import TestClient  # noqa: E402
from share_premium_service.database import Neo4jConnector

client = TestClient(app := main.app)

_spec = importlib.util.spec_from_file_location(
    "spr_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

USER = "user-spr"
OTHER_USER = "user-other"
BOOK_A = "book-spr-a"
BOOK_B = "book-spr-b"


def _headers(user=USER, book=None):
    h = {"X-User-Id": user}
    if book:
        h["X-Book-ID"] = book
    return h


def _clear():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


def _record_entry(company_id="co-1", headers=None, **params):
    return client.post(
        "/entries/record",
        params={
            "company_id": company_id,
            "entry_type": "issue",
            "shares_issued": 10000,
            "nominal_value": 1.0,
            "issue_price": 1.5,
            "share_class": "ordinary",
            "reference_id": "iss-1",
            "entry_date": "2026-09-01T10:00:00+00:00",
            **params,
        },
        headers=headers or _headers(),
    )


class TestEntries:
    def setup_method(self):
        _clear()

    def test_record_entry(self):
        r = _record_entry()
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["premium_amount"] == 5000.0
        assert data["user_id"] == USER

    def test_entries_persist_per_company(self):
        _record_entry(company_id="co-1")
        _record_entry(company_id="co-2")
        assert len(client.get("/entries", params={"company_id": "co-1"}, headers=_headers()).json()["entries"]) == 1
        assert len(client.get("/entries", headers=_headers()).json()["entries"]) == 2

    def test_book_isolation(self):
        _record_entry(company_id="co-a", headers=_headers(book=BOOK_A))
        _record_entry(company_id="co-a", headers=_headers(book=BOOK_B))
        a = client.get("/entries", params={"company_id": "co-a"}, headers=_headers(book=BOOK_A)).json()["entries"]
        assert len(a) == 1
        assert len(client.get("/entries", headers=_headers(user=OTHER_USER)).json()["entries"]) == 0


class TestSummary:
    def setup_method(self):
        _clear()
        # 5000 premium received
        _record_entry(company_id="co-1")
        # utilize 1000
        client.post(
            "/utilizations/record",
            params={
                "company_id": "co-1",
                "amount": 1000.0,
                "utilization_type": "bonus_issue",
                "description": "bonus issue",
                "utilization_date": "2026-09-02T10:00:00+00:00",
            },
            headers=_headers(),
        )
        # adjustment +250
        client.post(
            "/adjustments/create",
            params={
                "company_id": "co-1",
                "adjustment_type": "correction",
                "original_amount": 5000.0,
                "adjustment_amount": 250.0,
                "description": "correction",
                "adjustment_date": "2026-09-03T10:00:00+00:00",
            },
            headers=_headers(),
        )

    def test_summary_balance(self):
        s = client.get("/summary/co-1", headers=_headers()).json()
        assert s["total_premium_received"] == 5000.0
        assert s["total_utilized"] == 1000.0
        assert s["total_adjusted"] == 250.0
        assert s["current_balance"] == 4250.0

    def test_summary_book_isolated(self):
        # same company in another Book sees nothing
        _record_entry(company_id="co-1", headers=_headers(book=BOOK_B))
        s_b = client.get("/summary/co-1", headers=_headers(book=BOOK_B)).json()
        assert s_b["total_premium_received"] == 5000.0  # only its own entry
        assert s_b["total_utilized"] == 0
        assert s_b["current_balance"] == 5000.0

    def test_personal_view_spans_books(self):
        _record_entry(company_id="co-1", headers=_headers(book=BOOK_B))
        s = client.get("/summary/co-1", headers=_headers()).json()
        assert s["total_premium_received"] == 10000.0  # both Books' entries

    def test_utilizations_persist(self):
        us = client.get("/utilizations", params={"company_id": "co-1"}, headers=_headers()).json()["utilizations"]
        assert len(us) == 1
        assert us[0]["utilization_type"] == "bonus_issue"
