"""
Vimbai Benefits Administration Service - Test Suite
Covers plans, enrollments, and leave accruals plus Book (X-Book-ID) isolation.
"""

import importlib.util
import os

import main  # noqa: F401  (must come first: bootstraps the benefits_admin_service package)
from benefits_admin_service.database import Neo4jConnector
from fastapi.testclient import TestClient

client = TestClient(main.app)  # no context manager: startup (real Neo4j) never runs

BOOK_A = {"X-Book-ID": "book-aaa-111"}
BOOK_B = {"X-Book-ID": "book-bbb-222"}
USER = {"X-User-Id": "user-1"}

_spec = importlib.util.spec_from_file_location(
    "benefits_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession
FakeDriver = _fake_mod.FakeDriver

_fake_session = FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: FakeDriver(_fake_session))


def _create_plan(name="Medical Aid", plan_type="medical", **params):
    defaults = {"name": name, "plan_type": plan_type, "employer_contribution_pct": 5.0}
    defaults.update(params)
    return client.post("/plans", params=defaults, headers=USER)


class TestBenefitPlans:
    def test_health(self):
        assert client.get("/health").status_code == 200

    def test_create_and_list_plans(self):
        r = _create_plan()
        assert r.status_code == 200, r.text
        plan = r.json()
        assert plan["id"]
        assert plan["plan_type"] == "medical"
        assert plan["status"] == "active"

        listing = client.get("/plans", headers=USER)
        assert listing.status_code == 200
        assert any(p["id"] == plan["id"] for p in listing.json())

    def test_invalid_plan_type_rejected(self):
        r = _create_plan(name="Bad", plan_type="pizza")
        assert r.status_code == 422
        assert "Invalid plan type" in r.json()["detail"]

    def test_list_plans_filter_by_type(self):
        _create_plan(name="Pension A", plan_type="pension")
        _create_plan(name="Dental B", plan_type="dental")
        r = client.get("/plans", params={"plan_type": "pension"}, headers=USER)
        plans = r.json()
        assert len(plans) == 1
        assert plans[0]["plan_type"] == "pension"


class TestEnrollments:
    def test_enroll_employee(self):
        plan = _create_plan().json()
        r = client.post(
            "/enroll",
            params={"employee_id": "emp-1", "plan_id": plan["id"], "beneficiary": "Spouse"},
            headers=USER,
        )
        assert r.status_code == 200, r.text
        assert r.json()["status"] == "active"

        enrolls = client.get("/employee/emp-1/enrollments", headers=USER)
        assert enrolls.status_code == 200
        assert len(enrolls.json()) == 1
        assert enrolls.json()[0]["beneficiary"] == "Spouse"

    def test_enroll_unknown_plan_404(self):
        r = client.post("/enroll", params={"employee_id": "emp-1", "plan_id": "nope"}, headers=USER)
        assert r.status_code == 404

    def test_duplicate_enrollment_409(self):
        plan = _create_plan(name="Dup Plan").json()
        r1 = client.post("/enroll", params={"employee_id": "emp-dup", "plan_id": plan["id"]}, headers=USER)
        r2 = client.post("/enroll", params={"employee_id": "emp-dup", "plan_id": plan["id"]}, headers=USER)
        assert r1.status_code == 200
        assert r2.status_code == 409


class TestLeaveAccruals:
    def test_accrue_and_balance(self):
        r1 = client.post(
            "/leave/accrue",
            params={"employee_id": "emp-l", "leave_type": "annual", "period": "2026-08", "accrued_days": 2.5},
            headers=USER,
        )
        assert r1.status_code == 200, r1.text
        assert r1.json()["balance_days"] == 2.5

        r2 = client.post(
            "/leave/accrue",
            params={
                "employee_id": "emp-l",
                "leave_type": "annual",
                "period": "2026-09",
                "accrued_days": 2.5,
                "taken_days": 1.0,
            },
            headers=USER,
        )
        assert r2.status_code == 200
        assert r2.json()["balance_days"] == 4.0

        balances = client.get("/employee/emp-l/leave", headers=USER)
        assert len(balances.json()) == 2
        sick = client.get("/employee/emp-l/leave", params={"leave_type": "sick"}, headers=USER)
        assert sick.json() == []

    def test_invalid_leave_type_rejected(self):
        r = client.post(
            "/leave/accrue",
            params={"employee_id": "emp-l", "leave_type": "sabbatical", "period": "2026-09", "accrued_days": 1},
            headers=USER,
        )
        assert r.status_code == 422


class TestBookIsolation:
    def test_plans_scoped_to_book(self):
        plan = client.post(
            "/plans", params={"name": "BookA Plan", "plan_type": "medical"}, headers={**USER, **BOOK_A}
        ).json()
        assert plan["id"]

        listing_b = client.get("/plans", headers={**USER, **BOOK_B})
        assert all(p["id"] != plan["id"] for p in listing_b.json())

        listing_a = client.get("/plans", headers={**USER, **BOOK_A})
        assert any(p["id"] == plan["id"] for p in listing_a.json())
        stamped = [n for n in _fake_session.nodes if n["label"] == "BenefitPlan" and n["props"]["id"] == plan["id"]]
        assert stamped[0]["props"]["book_id"] == "book-aaa-111"

    def test_enroll_blocked_cross_book(self):
        plan = client.post(
            "/plans", params={"name": "XB Plan", "plan_type": "pension"}, headers={**USER, **BOOK_A}
        ).json()
        r = client.post(
            "/enroll",
            params={"employee_id": "emp-x", "plan_id": plan["id"]},
            headers={**USER, **BOOK_B},
        )
        assert r.status_code == 404

    def test_leave_scoped_to_book(self):
        client.post(
            "/leave/accrue",
            params={"employee_id": "emp-bk", "leave_type": "sick", "period": "2026-08", "accrued_days": 1.5},
            headers={**USER, **BOOK_A},
        )
        client.post(
            "/leave/accrue",
            params={"employee_id": "emp-bk", "leave_type": "sick", "period": "2026-08", "accrued_days": 3.0},
            headers={**USER, **BOOK_B},
        )
        leave_a = client.get("/employee/emp-bk/leave", headers={**USER, **BOOK_A}).json()
        leave_b = client.get("/employee/emp-bk/leave", headers={**USER, **BOOK_B}).json()
        assert len(leave_a) == 1
        assert leave_a[0]["accrued_days"] == 1.5
        assert len(leave_b) == 1
        assert leave_b[0]["accrued_days"] == 3.0

    def test_user_isolation(self):
        plan = _create_plan(name="UserIsolation Plan").json()
        other = {"X-User-Id": "user-2"}
        listing = client.get("/plans", headers=other)
        assert all(p["id"] != plan["id"] for p in listing.json())
