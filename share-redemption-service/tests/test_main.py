"""Share Redemption Service tests — Neo4j-backed, Book-scoped (fake harness)."""

import importlib.util
import os

import main  # noqa: F401  (must come first: bootstraps the package alias)
from fastapi.testclient import TestClient  # noqa: E402
from share_redemption_service.database import Neo4jConnector

client = TestClient(app := main.app)

_spec = importlib.util.spec_from_file_location(
    "srd_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

USER = "user-srd"
OTHER_USER = "user-other"
BOOK_A = "book-srd-a"
BOOK_B = "book-srd-b"


def _headers(user=USER, book=None):
    h = {"X-User-Id": user}
    if book:
        h["X-Book-ID"] = book
    return h


def _clear():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


def _initiate(company_id="co-1", headers=None, **params):
    return client.post(
        "/redemptions/initiate",
        params={
            "company_id": company_id,
            "share_class": "preference",
            "shares_redeemed": 1000,
            "nominal_value": 1.0,
            "redemption_price": 1.2,
            "redemption_date": "2026-09-01T10:00:00+00:00",
            "redemption_method": "proceeds",
            "authority_date": "2026-08-25T10:00:00+00:00",
            **params,
        },
        headers=headers or _headers(),
    )


class TestRedemptions:
    def setup_method(self):
        _clear()

    def test_initiate_proceeds(self):
        r = _initiate()
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["total_redemption_value"] == 1200.0
        assert data["status"] == "awaiting_crr"
        assert data["user_id"] == USER

        # CRR requirement recorded
        crrs = client.get("/crr-requirements", headers=_headers()).json()["crr_requirements"]
        assert len(crrs) == 1
        assert crrs[0]["minimum_crr_required"] == 1000.0

    def test_initiate_fresh_issue_no_crr(self):
        _initiate(redemption_method="fresh_issue")
        assert client.get("/crr-requirements", headers=_headers()).json()["crr_requirements"] == []
        r = client.get("/redemptions", headers=_headers()).json()["redemptions"]
        assert r[0]["status"] == "pending"

    def test_complete_persists_status(self):
        rid = _initiate().json()["id"]
        r = client.post(
            f"/redemptions/{rid}/complete",
            params={"statutory_declaration_date": "2026-09-10T10:00:00+00:00"},
            headers=_headers(),
        )
        assert r.status_code == 200
        assert r.json()["crr_created"] == 1000.0
        assert r.json()["redemption"]["status"] == "completed"

        # persisted
        listed = client.get("/redemptions", headers=_headers()).json()["redemptions"]
        assert listed[0]["status"] == "completed"

    def test_fresh_issue_and_details(self):
        rid = _initiate(redemption_method="fresh_issue").json()["id"]
        fi = client.post(
            f"/redemptions/{rid}/fresh-issue",
            params={
                "shares_issued": 2000,
                "issue_price": 1.0,
                "nominal_value": 1.0,
                "issue_date": "2026-09-05T10:00:00+00:00",
            },
            headers=_headers(),
        )
        assert fi.status_code == 200
        assert fi.json()["total_proceeds"] == 2000.0

        details = client.get(f"/redemptions/{rid}", headers=_headers()).json()
        assert len(details["fresh_issues"]) == 1
        assert details["crr_requirement"] is None

    def test_missing_redemption(self):
        r = client.get("/redemptions/nope", headers=_headers())
        assert r.json() == {"error": "Redemption not found"}

    def test_book_isolation(self):
        rid_a = _initiate(company_id="co-a", headers=_headers(book=BOOK_A)).json()["id"]
        _initiate(company_id="co-a", headers=_headers(book=BOOK_B))

        a = client.get("/redemptions", params={"company_id": "co-a"}, headers=_headers(book=BOOK_A)).json()
        assert len(a["redemptions"]) == 1

        # BOOK_B cannot complete BOOK_A's redemption
        r = client.post(
            f"/redemptions/{rid_a}/complete",
            params={"statutory_declaration_date": "2026-09-10T10:00:00+00:00"},
            headers=_headers(book=BOOK_B),
        )
        assert r.json() == {"error": "Redemption not found"}

        # CRR requirements are Book-scoped too
        assert len(client.get("/crr-requirements", headers=_headers(book=BOOK_A)).json()["crr_requirements"]) == 1
        assert len(client.get("/crr-requirements", headers=_headers(book=BOOK_B)).json()["crr_requirements"]) == 1
        assert len(client.get("/crr-requirements", headers=_headers(user=OTHER_USER)).json()["crr_requirements"]) == 0

    def test_personal_view_spans_books(self):
        _initiate(company_id="co-a", headers=_headers(book=BOOK_A))
        _initiate(company_id="co-b", headers=_headers(book=BOOK_B))
        assert len(client.get("/redemptions", headers=_headers()).json()["redemptions"]) == 2
