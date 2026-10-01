"""Book-scoping and persistence tests for tax-planning-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from tax_planning_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("tp_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "tp-user-1", "tp-user-2"
BOOK_A, BOOK_B = "tp-book-a", "tp-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _strategy(name="Capital allowance claim", **kw):
    p = {
        "name": name,
        "description": "Accelerate depreciation claims",
        "strategy_type": "deduction",
        "estimated_savings": 50000,
        "implementation_cost": 10000,
        "risk_level": "low",
        "timeframe": "short-term",
    }
    p.update(kw)
    return p


def test_create_list_persist():
    resp = client.post("/strategies", json=_strategy(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["id"]
    assert data["estimated_savings"] == 50000

    listed = client.get("/strategies", headers=H1).json()
    assert len(listed) == 1
    assert listed[0]["id"] == data["id"]
    # other user sees nothing
    assert client.get("/strategies", headers=H2).json() == []


def test_book_a_b_isolation():
    client.post("/strategies", json=_strategy(name="book-a"), headers=H1)
    hb = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    client.post("/strategies", json=_strategy(name="book-b"), headers=hb)
    assert len(client.get("/strategies", headers=H1).json()) == 1
    assert len(client.get("/strategies", headers=hb).json()) == 1
    # personal view spans both Books
    assert len(client.get("/strategies", headers=H1_PERSONAL).json()) == 2


def test_plan_is_pure_computation():
    payload = {
        "company_id": "co-tp",
        "fiscal_year": 2026,
        "current_taxable_income": 1000000,
        "current_tax": 250000,
        "strategies": [
            _strategy(estimated_savings=60000, implementation_cost=20000, risk_level="low"),
            {
                "name": "Offshore structure",
                "description": "x",
                "strategy_type": "structure",
                "estimated_savings": 80000,
                "implementation_cost": 30000,
                "risk_level": "high",
            },
        ],
    }
    resp = client.post("/plan", json=payload, headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["projected_tax"] == 110000  # 250000 - 140000
    assert data["total_savings"] == 140000
    assert data["net_benefit"] == 90000
    assert data["recommended_strategies"] == ["Capital allowance claim"]
    # plan does not persist anything
    assert client.get("/strategies", headers=H1).json() == []


def test_x_user_id_required():
    assert client.post("/strategies", json=_strategy()).status_code in (401, 403, 422)
    assert client.get("/strategies").status_code in (401, 403, 422)
