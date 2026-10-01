"""Book-scoping and persistence tests for sensitivity-analysis-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from sensitivity_analysis_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("sen_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "sen-user-1", "sen-user-2"
BOOK_A, BOOK_B = "sen-book-a", "sen-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _payload(company="co-sen", **kw):
    payload = {
        "company_id": company,
        "target_metric": "net_profit",
        "base_target_value": 100000,
        "variables": [
            {"name": "revenue", "base_value": 500000, "change_pct": 10},
            {"name": "costs", "base_value": 400000, "change_pct": 10},
        ],
        "change_steps": [-10, 0, 10],
    }
    payload.update(kw)
    return payload


def test_analyze_persists_and_scopes():
    resp = client.post("/analyze", json=_payload(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert len(data["results"]) == 6  # 2 variables * 3 steps
    assert data["most_sensitive_variable"] != ""
    assert data["book_id"] == BOOK_A

    stored = client.get("/analyses/co-sen", headers=H1).json()
    assert stored["total"] == 1
    assert len(stored["analyses"][0]["results"]) == 6
    assert stored["analyses"][0]["most_sensitive_variable"] == data["most_sensitive_variable"]

    # other user / other Book see nothing
    assert client.get("/analyses/co-sen", headers=H2).json()["total"] == 0
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/analyses/co-sen", headers=other_book).json()["total"] == 0


def test_book_a_b_isolation():
    client.post("/analyze", json=_payload(company="co-a"), headers=H1)
    client.post("/analyze", json=_payload(company="co-a"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/analyses/co-a", headers=H1).json()["total"] == 1
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/analyses/co-a", headers=other_book).json()["total"] == 1
    # personal spans books
    assert client.get("/analyses/co-a", headers=H1_PERSONAL).json()["total"] == 2


def test_elasticity_semantics_kept():
    # zero-impact step (change 0) yields elasticity 0
    payload = _payload(company="co-e", change_steps=[0])
    data = client.post("/analyze", json=payload, headers=H1).json()
    assert all(r["elasticity"] == 0 for r in data["results"])
    assert all(r["changed_value"] == r["base_value"] for r in data["results"])


def test_x_user_id_required():
    assert client.post("/analyze", json=_payload()).status_code in (401, 403, 422)
    assert client.get("/analyses/co-sen").status_code in (401, 403, 422)
