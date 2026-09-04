"""
Vimbai Expense Tracking Service - Test Suite
Covers CRUD for expenses plus Book (X-Book-ID) isolation.
"""

import importlib.util
import os

import main
from expense_tracking_service.database import Neo4jConnector
from fastapi.testclient import TestClient
from main import app

client = TestClient(app)  # no context manager: startup (real Neo4j) never runs

BOOK_A = {"X-Book-ID": "book-aaa-111"}
BOOK_B = {"X-Book-ID": "book-bbb-222"}
USER = {"X-User-Id": "user-1"}

_spec = importlib.util.spec_from_file_location(
    "expense_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession
FakeDriver = _fake_mod.FakeDriver

_fake_session = FakeSession()


def _setup_fake():
    # One shared session persists across requests, mirroring a real database.
    Neo4jConnector.get_driver = classmethod(lambda cls: FakeDriver(_fake_session))


_setup_fake()


def _expense_payload(company_id="comp-1", **overrides):
    payload = {
        "company_id": company_id,
        "employee_id": "emp-1",
        "category": "travel",
        "amount": 1500,
        "description": "Client visit",
        "vendor": "Airline",
    }
    payload.update(overrides)
    return payload


class TestExpenses:
    def test_health(self):
        assert client.get("/").status_code == 200

    def test_create_and_list(self):
        r = client.post("/expenses", json=_expense_payload(), headers={**USER, **BOOK_A})
        assert r.status_code == 200, r.text
        created = r.json()
        assert created["status"] == "pending"
        assert created["id"]

        listing = client.get("/expenses/comp-1", headers={**USER, **BOOK_A})
        assert listing.status_code == 200
        body = listing.json()
        assert body["total"] == 1
        assert body["expenses"][0]["id"] == created["id"]
        assert body["company_id"] == "comp-1"

    def test_list_filters(self):
        client.post(
            "/expenses", json=_expense_payload(company_id="comp-f", category="travel", amount=200), headers=USER
        )
        client.post(
            "/expenses", json=_expense_payload(company_id="comp-f", category="office", amount=300), headers=USER
        )
        r = client.get("/expenses/comp-f", params={"category": "travel"}, headers=USER)
        assert r.json()["total"] == 1
        assert r.json()["expenses"][0]["category"] == "travel"

        r2 = client.get("/expenses/comp-f", params={"status_filter": "pending"}, headers=USER)
        assert r2.json()["total"] == 2

    def test_approve_and_reject(self):
        created = client.post("/expenses", json=_expense_payload(), headers=USER).json()
        approve = client.put(f"/expenses/{created['id']}/approve", params={"approver": "manager-1"}, headers=USER)
        assert approve.status_code == 200
        assert approve.json()["status"] == "approved"
        assert approve.json()["approved_by"] == "manager-1"

        created2 = client.post("/expenses", json=_expense_payload(amount=50), headers=USER).json()
        reject = client.put(f"/expenses/{created2['id']}/reject", params={"reason": "out of policy"}, headers=USER)
        assert reject.status_code == 200
        assert reject.json()["status"] == "rejected"
        assert reject.json()["rejection_reason"] == "out of policy"

    def test_summary(self):
        client.post("/expenses", json=_expense_payload(company_id="comp-sum", amount=500), headers=USER)
        client.post(
            "/expenses", json=_expense_payload(company_id="comp-sum", category="office", amount=300), headers=USER
        )
        resp = client.get("/summary/comp-sum", headers=USER)
        assert resp.status_code == 200
        data = resp.json()
        assert data["total_amount"] == 800
        assert data["by_category"]["travel"] == 500
        assert data["by_category"]["office"] == 300
        assert data["total_expenses"] == 2

    def test_user_isolation(self):
        client.post("/expenses", json=_expense_payload(company_id="comp-u"), headers=USER)
        other = {"X-User-Id": "user-2"}
        listing = client.get("/expenses/comp-u", headers=other)
        assert listing.json()["total"] == 0


class TestBookIsolation:
    def test_expenses_scoped_to_book(self):
        a = client.post("/expenses", json=_expense_payload(company_id="comp-bk"), headers={**USER, **BOOK_A})
        assert a.status_code == 200
        listing_b = client.get("/expenses/comp-bk", headers={**USER, **BOOK_B})
        assert listing_b.status_code == 200
        assert listing_b.json()["total"] == 0

        listing_a = client.get("/expenses/comp-bk", headers={**USER, **BOOK_A})
        assert listing_a.json()["total"] == 1
        stamped = [n for n in _fake_session.nodes if n["label"] == "Expense"]
        assert stamped[0]["props"]["book_id"] == "book-aaa-111"

    def test_approve_blocked_cross_book(self):
        created = client.post("/expenses", json=_expense_payload(company_id="comp-x"), headers={**USER, **BOOK_A})
        r = client.put(
            f"/expenses/{created.json()['id']}/approve",
            params={"approver": "manager-1"},
            headers={**USER, **BOOK_B},
        )
        assert r.status_code == 404

    def test_summary_scoped_to_book(self):
        client.post("/expenses", json=_expense_payload(company_id="comp-bk2", amount=100), headers={**USER, **BOOK_A})
        client.post("/expenses", json=_expense_payload(company_id="comp-bk2", amount=400), headers={**USER, **BOOK_B})
        s_a = client.get("/summary/comp-bk2", headers={**USER, **BOOK_A}).json()
        assert s_a["total_amount"] == 100
        s_b = client.get("/summary/comp-bk2", headers={**USER, **BOOK_B}).json()
        assert s_b["total_amount"] == 400

    def test_personal_scope_sees_all_when_no_book(self):
        client.post("/expenses", json=_expense_payload(company_id="comp-no"), headers={**USER, **BOOK_A})
        client.post("/expenses", json=_expense_payload(company_id="comp-no", amount=5), headers={**USER, **BOOK_B})
        no_book = client.get("/expenses/comp-no", headers=USER).json()
        assert no_book["total"] == 2
