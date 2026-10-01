"""Book-scoping and persistence tests for subscription-plans-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from subscription_plans_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("sp_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession

_fake_session = FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "sp-user-1", "sp-user-2"
BOOK_A, BOOK_B = "sp-book-a", "sp-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _plan_payload(tier="professional", name="Pro Plan", price=199.0):
    return {
        "tier": tier,
        "name": name,
        "price_monthly": price,
        "features": ["Multi-company", "Advanced reporting"],
        "max_users": 50,
        "max_companies": 10,
        "api_calls_per_month": 10000,
    }


def test_create_plan_persists():
    resp = client.post("/plans", json=_plan_payload(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["book_id"] == BOOK_A
    assert data["tier"] == "professional"
    listed = client.get("/plans", headers=H1).json()
    assert len(listed) == 1
    assert listed[0]["name"] == "Pro Plan"
    assert listed[0]["features"] == ["Multi-company", "Advanced reporting"]


def test_plan_isolation_user_and_book():
    client.post("/plans", json=_plan_payload(), headers=H1)
    # other user sees nothing
    assert client.get("/plans", headers=H2).json() == []
    # other Book sees nothing
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/plans", headers=other_book).json() == []
    # personal view spans Books
    assert len(client.get("/plans", headers=H1_PERSONAL).json()) == 1


def test_subscribe_requires_visible_plan():
    plan = client.post("/plans", json=_plan_payload(), headers=H1).json()
    sub = client.post(
        "/subscribe",
        params={"company_id": "comp-1", "plan_id": plan["id"], "cycle": "monthly"},
        headers=H1,
    )
    assert sub.status_code == 200, sub.text
    data = sub.json()
    assert data["company_id"] == "comp-1"
    assert data["status"] == "active"
    assert data["book_id"] == BOOK_A
    assert data["tier"] == "professional"
    assert data["current_period_end"]

    # other user cannot subscribe to this plan
    blocked = client.post(
        "/subscribe",
        params={"company_id": "comp-1", "plan_id": plan["id"]},
        headers=H2,
    )
    assert blocked.status_code == 404

    # other Book cannot subscribe to this Book's plan
    blocked_book = client.post(
        "/subscribe",
        params={"company_id": "comp-1", "plan_id": plan["id"]},
        headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
    )
    assert blocked_book.status_code == 404


def test_subscribe_unknown_plan_404():
    resp = client.post(
        "/subscribe",
        params={"company_id": "comp-1", "plan_id": "nope"},
        headers=H1,
    )
    assert resp.status_code == 404


def test_subscriptions_scoped():
    plan = client.post("/plans", json=_plan_payload(), headers=H1).json()
    client.post(
        "/subscribe",
        params={"company_id": "comp-1", "plan_id": plan["id"], "cycle": "annual"},
        headers=H1,
    )
    subs = client.get("/subscriptions/comp-1", headers=H1).json()
    assert len(subs) == 1
    assert subs[0]["billing_cycle"] == "annual"
    assert client.get("/subscriptions/comp-1", headers=H2).json() == []
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/subscriptions/comp-1", headers=other_book).json() == []
    assert len(client.get("/subscriptions/comp-1", headers=H1_PERSONAL).json()) == 1


def test_upgrade_pure_computation():
    resp = client.post(
        "/upgrade",
        json={
            "company_id": "comp-1",
            "current_plan": "basic",
            "target_plan": "professional",
            "current_period_end": "2026-12-01T00:00:00Z",
            "prorate": True,
        },
    )
    assert resp.status_code == 200
    data = resp.json()
    assert data["current_plan"] == "basic"
    assert data["target_plan"] == "professional"
    assert data["new_billing_amount"] == 199
    assert data["proration_amount"] > 0


def test_x_user_id_required():
    assert client.post("/plans", json=_plan_payload()).status_code in (401, 403, 422)
